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
//! # Data flow (per-layer planes — the stage-4 publish)
//!
//! There is no destructive read-modify-publish: each material layer owns
//! its planes, `(Layer(id), tile, channel)`, and a refresh REPLACES them
//! whole. Per dirty material layer, per present tile, per declared +
//! enabled + allowed-blend channel: eval once at the SET size, resample
//! to the channel size, convert straight → premultiplied, `publish_plane`.
//! The per-channel blend/opacity/enable apply at COMPOSITE time (the
//! shell's snapshot carries one entry per contributing channel, §5.2).
//!
//! # v1 boundaries (warts 1–3 CLOSED by per-layer planes; wart 4 lands in
//! stage 5)
//!
//! * ~~Stack order is not preserved~~ — CLOSED: the composite folds
//!   entries in stack order on every flush, and a material's plane holds
//!   only ITS output. Reorder marks `Full` (§4.2). Pinned by §8 T5 (115,
//!   then 110 — the destructive path's 51 never appears).
//! * ~~Re-eval compounds~~ — CLOSED: a refresh REPLACES the plane, and the
//!   composite of identical plane bytes is identical. Pinned by §8 T6
//!   (170, 170, never 144). The driver tests that pinned the destructive
//!   publish (`refresh_order_is_bottom_to_top_171_and_reversed_187`,
//!   `reeval_compounds_the_documented_boundary`) are RETIRED with the
//!   path they pinned.
//! * ~~Nothing un-publishes~~ — CLOSED: hide skips the entry, a disabled
//!   channel carries `pixels: None`, a delete DROPS the eval-owned planes
//!   (no tombstone — revive re-evaluates from the `Insert` params).
//!   Pinned by §8 T7.
//! * ~~Anchors read the flattened channel (self-feedback on re-eval)~~ —
//!   CLOSED (stage 5): an anchor on layer `A`, channel `c` resolves to the
//!   composite of the stack from the bottom (Base included) THROUGH `A` —
//!   the downward partial flatten, on the CPU oracle so the bytes are
//!   bit-exact (`PaintState::read_anchor_prefix`). A material may anchor
//!   only layers strictly below it: itself is [`MaterialError::SelfAnchor`],
//!   a layer above is [`MaterialError::ForwardAnchor`] — both checked
//!   before eval ([`validate_anchor_order`]), so self-feedback is
//!   impossible by construction. Pinned by §8 T8.
//! * Instances render nothing — STAGE 5 (they read their source's planes).
//!   Until then `pixels: None`, a known v1 state, not a regression.
//! * The layer's own `blend_mode` is INERT on Material layers (carried,
//!   never read); layer `opacity` multiplies every channel's opacity at
//!   SNAPSHOT time (so opacity edits propagate without a re-eval) and the
//!   layer mask gates every entry at composite time.
//! * Eval runs ONCE per material per refresh at the SET size (never a
//!   literal) and resamples per channel — never once per channel size.
//! * A state created by the project-open path is seeded CLEAN (the saved
//!   sidecars already hold the published result; re-publishing on open
//!   would double-count over the legacy Base). Runtime states start dirty.
//!   Dormant v1 fills/materials (provenance: pre-v2 manifest) stay clean
//!   until an explicit edit; v2 planeless layers seed DIRTY (§6.3 — the
//!   shell decides, `ensure_open` + `mark_dirty` are the arms).
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
//! * A `publish_channel` plane-budget refusal surfaces as
//!   [`MaterialResult::BudgetBlocked`] (the state records the bytes and
//!   stays dirty — it retries when `plane_bytes()` falls or params
//!   change, never every frame). Any other publish failure is a loud
//!   whole-material error, never a silent partial plane.

use std::collections::{BTreeSet, HashMap, HashSet};

use umber_core::layers::{BlendMode, Layer, LayerKind, MaterialParamValue};
use umber_core::MaterialParams;
use umber_gpu::f32_to_u8;

use crate::paint_state::{PaintRefusal, SnapshotMaterialChannel};
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
    /// The material anchors ITSELF (§5.3): a layer may anchor only layers
    /// strictly below it, so self-feedback is impossible by construction.
    /// Whole-material, badged, never silent.
    #[error("material layer {layer} anchors itself")]
    SelfAnchor {
        /// The layer that cannot evaluate.
        layer: u64,
    },
    /// The material anchors a layer ABOVE it in the stack (§5.3): the
    /// anchor's prefix would include the material's own output.
    #[error("material layer {layer} anchors layer {anchor}, which sits above it")]
    ForwardAnchor {
        /// The layer that cannot evaluate.
        layer: u64,
        /// The anchored layer above it.
        anchor: u64,
    },
    /// The plane budget refused a publish (§1.3): the total
    /// `plane_bytes()` the materialization would have reached. The layer
    /// stays dirty and retries per §1.3 — never a silent partial plane.
    #[error("material layer {layer} plane budget refused ({bytes} bytes)")]
    BudgetBlocked {
        /// The layer that cannot publish.
        layer: u64,
        /// The total the materialization would have reached.
        bytes: u64,
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
    /// The plane budget refused a publish (§1.3): nothing further staged,
    /// the state records the bytes and stays dirty (retries when
    /// `plane_bytes()` falls or params change).
    BudgetBlocked {
        /// The total `plane_bytes()` the materialization would have reached.
        bytes: u64,
    },
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
/// UNNORMALIZED, exactly as the stack compositor's pixel primitive does —
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
    /// The plane-budget refusal blocking this layer, if any (see the fill
    /// precedent's `budget_blocked`): the total `plane_bytes()` the
    /// materialization would have reached.
    budget_blocked: Option<u64>,
    /// The last refresh's contributing channels (§5.2): one entry per
    /// DECLARED + enabled + allowed-blend channel, with the CHANNEL-level
    /// blend/opacity (the shell forms `layer.opacity × channel.opacity`
    /// at snapshot time, so opacity edits propagate without a re-eval).
    /// Refreshed whole on every successful refresh.
    composite_channels: Vec<SnapshotMaterialChannel>,
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
            budget_blocked: None,
            composite_channels: Vec::new(),
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

    /// The plane-budget refusal blocking this layer, if any.
    pub fn budget_blocked(&self) -> Option<u64> {
        self.budget_blocked
    }

    /// The last refresh's contributing channels for the shell's composite
    /// snapshot (§5.2): channel-level blend/opacity per declared + enabled
    /// + allowed-blend channel.
    pub fn composite_channels(&self) -> &[SnapshotMaterialChannel] {
        &self.composite_channels
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
    /// Any newly-dirtying change clears a budget block (new inputs may fit;
    /// the retry below re-blocks loudly, never silently).
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
            self.budget_blocked = None;
        }
        if definition != self.definition {
            self.definition = definition.to_string();
            self.full_eval = true;
            self.dirty = true;
            self.budget_blocked = None;
        }
        if set_size != self.set_size {
            self.set_size = set_size;
            self.full_eval = true;
            self.dirty = true;
            self.budget_blocked = None;
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

    /// Requeues budget-blocked layers whose block may have cleared (the
    /// fill precedent's `retry_budget_freed`): a state blocked at
    /// `blocked_bytes` retries when the session now holds fewer plane
    /// bytes. Retrying every frame would spam; the shell calls this when
    /// bytes fall (params changes requeue through `ensure`).
    pub fn retry_budget_freed(&mut self, plane_bytes: u64) {
        let retry: Vec<u64> = self
            .states
            .iter()
            .filter(|(_, state)| {
                state
                    .budget_blocked
                    .is_some_and(|blocked| plane_bytes < blocked)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in retry {
            if let Some(state) = self.states.get_mut(&id) {
                state.budget_blocked = None;
                state.dirty = true;
            }
            if !self.dirty.contains(&id) {
                self.dirty.push(id);
            }
        }
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
/// boundary: tests drive an in-memory implementation, the shell adapts the
/// paint session at the call site — its anchor reads never
/// side-effect-create, its publishes materialize the layer's own plane).
pub trait MaterialTileIo {
    /// The set's paint-target size (the eval resolution — never a literal).
    fn set_size(&self) -> (u32, u32);
    /// `channel`'s paint-target size (its override, else the set's size).
    fn channel_size(&self, channel: &str) -> (u32, u32);
    /// The tiles to refresh, ascending.
    fn tiles(&self) -> Vec<u16>;
    /// Uploads the resampled, premultiplied output into the layer's OWN
    /// `(Layer(layer), tile, channel)` plane (§5.2), materializing it. A
    /// budget refusal propagates (the driver records `budget_blocked`
    /// and writes nothing further).
    fn publish_channel(
        &mut self,
        layer: u64,
        tile: u16,
        channel: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<(), PaintRefusal>;
    /// One anchor's channel image for `tile`: the stack composited from
    /// the bottom THROUGH `layer` (the downward partial flatten, §5.3), or
    /// `None` when the source layer was deleted (an eval error, never a
    /// silent default). The driver has already refused self and forward
    /// anchors, so `layer` sits strictly below the material.
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
            layers,
            &params,
            definitions,
            set_channels,
            registry,
            io,
        );
        // A budget block keeps the dirty flag (the retry contract —
        // `retry_budget_freed` / param edits requeue) but leaves the
        // queue: retrying every frame would spam. Every other attempt
        // (published or failed) clears it.
        if !matches!(result, MaterialResult::BudgetBlocked { .. }) {
            state.dirty = false;
        }
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
/// plane publish. Reads `state`'s caches and engine dirty set; updates
/// them on success.
#[allow(clippy::too_many_arguments)]
fn refresh_one(
    state: &mut MaterialLayerState,
    layer: &Layer,
    layers: &[Layer],
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
    // §5.3: anchors point strictly DOWN the stack, checked before any eval
    // or anchor read (a forward/self anchor never reaches `anchor_image`).
    if let Err(err) = validate_anchor_order(layer_id, &collect_anchor_refs(&graph), layers) {
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
                return map_publish_error(state, err);
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
                return map_publish_error(state, err);
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
    // The shell's composite snapshot reads these (channel-level
    // blend/opacity per contributing channel — the layer opacity multiplies
    // in at snapshot time).
    state.composite_channels = active
        .iter()
        .map(|plan| SnapshotMaterialChannel {
            channel: plan.output.channel.clone(),
            blend: plan.blend,
            opacity: plan.opacity,
        })
        .collect();
    state.budget_blocked = None;
    state.engine_dirty.clear();
    state.full_eval = false;
    MaterialResult::Refreshed {
        channels,
        ignored_undeclared,
    }
}

/// Maps a per-tile publish failure onto the refresh outcome: a plane
/// budget records the bytes on the state (which stays dirty — the drain
/// keeps it dirty and unqueued, so it retries only when bytes fall or
/// params change) and reports [`MaterialResult::BudgetBlocked`]; anything
/// else is a whole-material [`MaterialResult::Failed`].
fn map_publish_error(state: &mut MaterialLayerState, err: MaterialError) -> MaterialResult {
    match err {
        MaterialError::BudgetBlocked { bytes, .. } => {
            state.budget_blocked = Some(bytes);
            MaterialResult::BudgetBlocked { bytes }
        }
        other => MaterialResult::Failed(other),
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

/// §5.3's anchor-order rule: every anchor must name a layer strictly BELOW
/// `layer` in `layers` (stack order, bottom-to-top). An anchor on `layer`
/// itself is [`MaterialError::SelfAnchor`]; one on a layer above it is
/// [`MaterialError::ForwardAnchor`]. An anchor naming a layer absent from
/// `layers` passes here: the io's `anchor_image` returns `None` for it and
/// the eval fails loud (the deleted-source contract, unchanged). Anchors
/// are checked in their deterministic (sorted) order, so the first
/// violation reported is stable.
fn validate_anchor_order(
    layer: u64,
    anchors: &[AnchorRef],
    layers: &[Layer],
) -> Result<(), MaterialError> {
    let own = layers.iter().position(|l| l.id == layer);
    for anchor in anchors {
        if anchor.layer_id == layer {
            return Err(MaterialError::SelfAnchor { layer });
        }
        let at = layers.iter().position(|l| l.id == anchor.layer_id);
        if let (Some(own), Some(at)) = (own, at) {
            if at > own {
                return Err(MaterialError::ForwardAnchor {
                    layer,
                    anchor: anchor.layer_id,
                });
            }
        }
    }
    Ok(())
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

/// Converts straight-alpha eval bytes to the premultiplied plane bytes
/// (§5.2): texel = `(c · α, α)`, quantized by `f32_to_u8`. Every writer
/// into a layer-owned plane keeps `p ≤ a` (§3's invariant); opaque texels
/// pass through byte-identical. Shared with the mesh-map import publish
/// (straight PNG → plane, §5.6).
pub fn straight_to_premultiplied(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let alpha = f32::from(px[3]) / 255.0;
        out.push(f32_to_u8(f32::from(px[0]) / 255.0 * alpha));
        out.push(f32_to_u8(f32::from(px[1]) / 255.0 * alpha));
        out.push(f32_to_u8(f32::from(px[2]) / 255.0 * alpha));
        out.push(f32_to_u8(alpha));
    }
    out
}

/// The per-tile publish: resample each active output to its channel size
/// and write it, premultiplied, into the layer's OWN plane (a refresh
/// REPLACES the plane whole — re-eval is idempotent, hides un-publish).
/// Records each channel's outcome in `statuses` (tile-independent skips
/// land once; publishes accumulate). The mask is NOT read here (it gates
/// at composite time); the flattened channel is NOT read (wart 2's
/// read-modify-publish is retired with the destructive path).
///
/// # Errors
///
/// [`MaterialError::Internal`] when a successfully-evaluated node has no
/// output (allegedly unreachable — loud, never wrong pixels), and
/// [`MaterialError::BudgetBlocked`] when the plane budget refuses (the
/// driver records the bytes and stops the tile — earlier channels'
/// planes stand, and the retry replaces them whole).
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
        let plane = straight_to_premultiplied(&src);
        io.publish_channel(layer.id, tile, &channel, cw, ch, plane)
            .map_err(|refusal| match refusal {
                PaintRefusal::PlaneBudget { bytes } => MaterialError::BudgetBlocked {
                    layer: layer.id,
                    bytes,
                },
                // `publish_channel`'s only other refusal arms a gate the
                // driver never trips (unpaintable layers never evaluate):
                // loud, never wrong pixels.
                other => MaterialError::Internal {
                    layer: layer.id,
                    message: format!("publish refused: {other:?}"),
                },
            })?;
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
        LayerMask, LayerStack, MaterialChannel, MaterialParamOverride, MaterialParamValue,
    };
    use umber_graph::mtlx::{painter_nodedefs, to_mtlx};
    use umber_graph::{Edge, Node};

    const SET_CHANNELS: &[&str] = &["baseColor", "roughness", "metallic", "normal", "height"];

    fn set_channels() -> Vec<String> {
        SET_CHANNELS.iter().map(|s| s.to_string()).collect()
    }

    /// Scripted in-memory tile store: the headless stand-in for the paint
    /// session (sizes, tiles, anchors, mesh maps — all explicit; masks
    /// gate at composite time now, so the store carries none).
    #[derive(Debug, Default)]
    struct FakeTileIo {
        set_size: (u32, u32),
        channel_sizes: HashMap<String, (u32, u32)>,
        tile_list: Vec<u16>,
        tiles: HashMap<(u16, String), (u32, u32, Vec<u8>)>,
        anchors: AnchorPlanes,
        meshes: HashMap<String, (u32, u32, Vec<u8>)>,
        publish_log: Vec<(u16, String)>,
        anchor_log: Vec<(u16, u64, String)>,
        mesh_log: Vec<String>,
        /// When set, `publish_channel` refuses with
        /// `PlaneBudget { bytes }` (the §1.3 path).
        refuse_publish: Option<u64>,
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

        fn publish_channel(
            &mut self,
            // The fake store is not owner-keyed (one tile map for all
            // layers — owner routing is the paint session's pin, §8 T9),
            // so the layer id rides along unrecorded.
            _layer: u64,
            tile: u16,
            channel: &str,
            width: u32,
            height: u32,
            rgba: Vec<u8>,
        ) -> Result<(), PaintRefusal> {
            self.publish_log.push((tile, channel.to_string()));
            if let Some(bytes) = self.refuse_publish {
                return Err(PaintRefusal::PlaneBudget { bytes });
            }
            self.tiles
                .insert((tile, channel.to_string()), (width, height, rgba));
            Ok(())
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

    /// The stack's live view of `layer_id` (the drain must see post-edit
    /// params — a stale snapshot would composite pre-edit plans).
    fn layer_snapshot(stack: &LayerStack, layer_id: u64) -> Layer {
        stack.layer(layer_id).expect("layer exists").clone()
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
        // Metallic: byte-identical, never published (the io seam has no
        // channel read since stage 5 — "never read" holds by construction).
        assert_eq!(io.tile_bytes(1001, "metallic"), before_metallic);
        assert!(
            io.publish_log.iter().all(|(_, c)| c != "metallic"),
            "undeclared channels are never published: {:?}",
            io.publish_log
        );
    }

    #[test]
    fn opacity_product_golden_publishes_src_for_composite() {
        // §5.2a (stage-4 shape): the driver publishes the resampled SRC —
        // opaque 100 stays `(100,100,100,255)` (premultiply is identity on
        // opaque) — and the 0.5 × 0.5 opacity product rides the composite
        // snapshot (see `material_composite_channels` below and §8 T13's
        // stack: 200·(1 − 0.25·0.607843) = 169.608 → 170 THERE, never in
        // the plane). The r1 two-level (97) and lone-0.5 (139) composite
        // sentinels retire with the destructive path (T13 pins 170 at the
        // composite level instead).
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
        let (states, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "baseColor"),
            ChannelStatus::Published
        );
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![100, 100, 100, 255]);
        // ... and the snapshot carries the product for composite time.
        let channels = states
            .state(layer.id)
            .expect("state tracked")
            .composite_channels();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].channel, "baseColor");
        assert_eq!(channels[0].blend, BlendMode::Multiply);
        assert_eq!(channels[0].opacity, 0.5);
    }

    /// §8 T13: one channel, Multiply, layer 0.5 × channel 0.5 over
    /// `L0 = 200` gives 170 — the dynmat 2a golden reproduced through the
    /// STACK composite instead of the destructive publish.
    #[test]
    fn material_entry_composites_170_through_the_stack() {
        use umber_gpu::compositor::{composite_stack_cpu, CpuStackLayer};
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
        let (states, _) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        let plane = io.tile_bytes(1001, "baseColor");
        assert_eq!(plane, vec![100, 100, 100, 255]);
        // The shell forms `layer.opacity × channel.opacity` at snapshot
        // time (0.5 × 0.5 = 0.25) and folds the entry over L0 = 200.
        let snapshot = states
            .state(layer.id)
            .expect("state tracked")
            .composite_channels();
        assert_eq!(snapshot.len(), 1);
        let l0 = [200, 200, 200, 255];
        let out = composite_stack_cpu(
            &[
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    rgba: Some(&l0),
                    mask: None,
                    premultiplied: true,
                },
                CpuStackLayer {
                    blend_mode: snapshot[0].blend,
                    opacity: layer.opacity * snapshot[0].opacity,
                    visible: true,
                    rgba: Some(&plane),
                    mask: None,
                    premultiplied: true,
                },
            ],
            1,
            1,
        )
        .expect("oracle composites");
        assert_eq!(out, vec![170, 170, 170, 255]);
    }

    #[test]
    fn triplet_modes_route_through_blend_rgb_with_pinned_bytes() {
        // §5.3: the eval still routes Saturation through `blend_rgb` (a
        // scalar-primitive routing would fail loud and publish nothing) —
        // but the PLANE holds the graph's src bytes now (red, opaque):
        // blend modes apply at composite time, so [128, 0, 0] moves to
        // the stack level (see §8 T5's Multiply order pin).
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
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);
    }

    #[test]
    fn normal_combine_pins_whiteout_bytes_not_identity() {
        // §5.3: the eval resolves tilt-over-tilt through NormalCombine to
        // the Whiteout src bytes [218, 128, 218] — NOT a bypass/identity
        // shortcut (which would hand back something else) and NOT nothing
        // (the scalar path publishes nothing). The Whiteout composite
        // ([248, 128, 170]) now forms at composite time, through the stack
        // like every other mode. Byte 128 decodes to +0.0039, not exactly
        // flat, so this also pins the non-flat math.
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
        assert_eq!(io.tile_bytes(1001, "normal"), vec![218, 128, 218, 255]);
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
    fn mask_nearest_resample_pins_blocks_and_publish_holds_src() {
        // §5.7: a 2×2 mask nearest-resampled to 4×4 replicates 2×2 blocks,
        // and a set-sized mask resampled to an overridden channel is
        // exact (no InvalidMaskData class of failure — the headless
        // analogue is exact output bytes).
        assert_eq!(
            resample_mask_nearest(&[0, 255, 255, 0], 2, 2, 4, 4),
            vec![0, 0, 255, 255, 0, 0, 255, 255, 255, 255, 0, 0, 255, 255, 0, 0,]
        );

        // End to end: set 1×1, roughness overridden to 2×2. The graph
        // evaluates once at the set size (1×1 src 100), the output
        // resamples to 2×2, and the plane holds the premultiplied src
        // (blend/opacity/mask all apply at composite time now — the
        // composite half is §8 T5/T13's stack, not this driver).
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
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.override".into(),
            solid_def([gray100, gray100, gray100], "roughness"),
        );

        let mut io = FakeTileIo {
            set_size: (1, 1),
            channel_sizes: HashMap::from([("roughness".to_string(), (2, 2))]),
            tile_list: vec![1001],
            tiles: HashMap::from([(
                (1001, "roughness".to_string()),
                (2, 2, [200u8, 200, 200, 255].repeat(4).to_vec()),
            )]),
            ..Default::default()
        };
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "roughness"),
            ChannelStatus::Published
        );
        assert_eq!(
            io.tile_bytes(1001, "roughness"),
            [100u8, 100, 100, 255].repeat(4).to_vec(),
            "the plane holds the resampled src (opaque: premultiply is identity)"
        );

        // A mask on the layer changes NOTHING at publish time (it gates
        // the composite entry instead): the plane holds the same src.
        let mut gated = layer.clone();
        gated.mask = Some(LayerMask {
            name: "M".into(),
            enabled: true,
        });
        let mut io = FakeTileIo {
            set_size: (1, 1),
            channel_sizes: HashMap::from([("roughness".to_string(), (2, 2))]),
            tile_list: vec![1001],
            tiles: HashMap::from([(
                (1001, "roughness".to_string()),
                (2, 2, [200u8, 200, 200, 255].repeat(4).to_vec()),
            )]),
            ..Default::default()
        };
        let (_, reports) = drive(std::slice::from_ref(&gated), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, gated.id, "roughness"),
            ChannelStatus::Published
        );
        assert_eq!(
            io.tile_bytes(1001, "roughness"),
            [100u8, 100, 100, 255].repeat(4).to_vec(),
            "masks gate at composite time, never at publish"
        );
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
        let mut definitions = HashMap::new();
        definitions.insert("umber.counted".into(), definition);
        let mut io = FakeTileIo::flat((2, 2), 1001, "baseColor", [0, 0, 0, 255]);

        // The layer snapshots the DRAIN sees must track the stack: each
        // edit lands via `set_material` and the slice is rebuilt — the
        // refresh reads its channel plans from `layer.material`, so a
        // stale snapshot would composite with pre-edit plans.
        let mut states = drive_states(
            std::slice::from_ref(&layer_snapshot(&stack, layer.id)),
            &definitions,
            (2, 2),
        );
        let drain = |states: &mut MaterialLayerStates, io: &mut FakeTileIo, layers: &[Layer]| {
            refresh_dirty_materials(states, layers, &definitions, &set_channels(), &registry, io)
        };
        drain(
            &mut states,
            &mut io,
            std::slice::from_ref(&layer_snapshot(&stack, layer.id)),
        );
        assert_eq!(run_counts(&counts), [1, 1]);
        assert!(io
            .tile_bytes(1001, "baseColor")
            .chunks_exact(4)
            .all(|px| px == [10, 0, 0, 255]));

        // Channel-only edit (blend flip): the graph does not re-run —
        // warm caches serve byte-identical outputs — and the plane is
        // republished identically (blend applies at composite time now).
        // The edit lands through the stack so the drain's snapshot sees
        // it, and the state's composite channels carry the new blend for
        // the shell's snapshot.
        let mut changed = layer.material.clone().unwrap();
        changed.channels[0].blend = BlendMode::Multiply;
        stack.set_material(layer.id, changed.clone());
        states.ensure(layer.id, &changed, &definitions["umber.counted"], (2, 2));
        drain(
            &mut states,
            &mut io,
            std::slice::from_ref(&layer_snapshot(&stack, layer.id)),
        );
        assert_eq!(
            run_counts(&counts),
            [1, 1],
            "channel-only edits reuse the caches"
        );
        assert!(io
            .tile_bytes(1001, "baseColor")
            .chunks_exact(4)
            .all(|px| px == [10, 0, 0, 255]));
        assert_eq!(
            states
                .state(layer.id)
                .expect("state tracked")
                .composite_channels()[0]
                .blend,
            BlendMode::Multiply,
            "the new blend rides the snapshot, not a re-composite"
        );

        // Override edit on node 2: node 1 stays at 1 run, node 2 re-runs.
        // (Node 2 echoes its input, so the bytes are unchanged — the pin is
        // the run counts, not the pixels.)
        changed.params = vec![MaterialParamOverride {
            node_id: 2,
            param: "bias".into(),
            value: MaterialParamValue::Int(5),
        }];
        changed.channels[0].blend = BlendMode::Normal;
        stack.set_material(layer.id, changed.clone());
        states.ensure(layer.id, &changed, &definitions["umber.counted"], (2, 2));
        drain(
            &mut states,
            &mut io,
            std::slice::from_ref(&layer_snapshot(&stack, layer.id)),
        );
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
        stack.set_material(layer.id, changed.clone());
        states.ensure(layer.id, &changed, &definitions["umber.counted"], (2, 2));
        drain(
            &mut states,
            &mut io,
            std::slice::from_ref(&layer_snapshot(&stack, layer.id)),
        );
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

    /// A one-output definition reading an anchor on `(anchor_layer,
    /// baseColor)` straight into baseColor.
    fn anchored_def(anchor_layer: i32) -> String {
        let mut graph = Graph::new();
        graph.add_node(Node {
            id: 1,
            node_def: "anchor".into(),
            params: vec![
                ("layer_id".into(), ParamValue::Int(anchor_layer)),
                ("channel".into(), ParamValue::Asset("baseColor".into())),
            ],
            canvas: None,
        });
        graph.add_node(output_node(2, "baseColor"));
        graph.add_edge(edge(1, 2));
        to_mtlx(&graph, &painter_nodedefs())
    }

    #[test]
    fn self_anchor_is_a_typed_error_and_reads_nothing() {
        // §8 T8 (stage 5): `anchor(M)` → `SelfAnchor { layer: M }`, refused
        // BEFORE any anchor read (self-feedback impossible by construction).
        let mut stack = LayerStack::new();
        let id = stack.add_layer("M", LayerKind::Material);
        let params = channel_params("umber.self", vec![MaterialChannel::named("baseColor")]);
        stack.set_material(id, params);
        let layer = stack.layer(id).unwrap().clone();
        let mut definitions = HashMap::new();
        definitions.insert("umber.self".into(), anchored_def(id as i32));
        let mut io = FakeTileIo {
            set_size: (1, 1),
            tile_list: vec![1001],
            anchors: HashMap::from([((1001, id, "baseColor".into()), (1, 1, vec![9, 9, 9, 255]))]),
            ..Default::default()
        };
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        match &reports[0].result {
            MaterialResult::Failed(MaterialError::SelfAnchor { layer }) => assert_eq!(*layer, id),
            other => panic!("expected SelfAnchor, got {other:?}"),
        }
        assert!(io.anchor_log.is_empty(), "a self anchor must never be read");
        assert!(
            io.publish_log.is_empty(),
            "a refused material publishes nothing"
        );
    }

    #[test]
    fn forward_anchor_is_a_typed_error_and_below_anchor_evaluates() {
        // §8 T8: an anchor on a layer ABOVE `M` → `ForwardAnchor`; the same
        // definition anchored on a layer BELOW evaluates and publishes the
        // anchor's bytes. Stack (bottom-to-top): below, M, above.
        let mut stack = LayerStack::new();
        let below = stack.add_layer("below", LayerKind::Paint);
        let m = stack.add_layer("M", LayerKind::Material);
        let above = stack.add_layer("above", LayerKind::Paint);
        let params = channel_params("umber.fwd", vec![MaterialChannel::named("baseColor")]);
        stack.set_material(m, params);
        let layers = stack.layers.clone();
        let mut definitions = HashMap::new();
        definitions.insert("umber.fwd".into(), anchored_def(above as i32));
        let anchors = HashMap::from([
            (
                (1001, above, "baseColor".into()),
                (1, 1, vec![1, 2, 3, 255]),
            ),
            (
                (1001, below, "baseColor".into()),
                (1, 1, vec![40, 50, 60, 255]),
            ),
        ]);
        let mut io = FakeTileIo {
            set_size: (1, 1),
            tile_list: vec![1001],
            anchors: anchors.clone(),
            ..Default::default()
        };
        let (_, reports) = drive(&layers, &definitions, &mut io);
        let report = reports.iter().find(|r| r.layer == m).expect("M reports");
        match &report.result {
            MaterialResult::Failed(MaterialError::ForwardAnchor { layer, anchor }) => {
                assert_eq!((*layer, *anchor), (m, above));
            }
            other => panic!("expected ForwardAnchor, got {other:?}"),
        }
        assert!(
            io.anchor_log.is_empty(),
            "a forward anchor must never be read"
        );

        // Control: anchored BELOW → published with the anchor's bytes.
        definitions.insert("umber.fwd".into(), anchored_def(below as i32));
        let mut io = FakeTileIo {
            set_size: (1, 1),
            tile_list: vec![1001],
            anchors,
            ..Default::default()
        };
        let (_, reports) = drive(&layers, &definitions, &mut io);
        assert_eq!(
            status_of(&reports, m, "baseColor"),
            ChannelStatus::Published
        );
        assert_eq!(io.anchor_log, vec![(1001, below, "baseColor".to_string())]);
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![40, 50, 60, 255]);
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
        assert!(io.anchor_log.is_empty(), "invisible layers read nothing");

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
            io.publish_log.iter().all(|(_, c)| c != "roughness"),
            "undeclared channels are never published even when listed"
        );
    }

    /// §1.3's budget path: a refused publish records `budget_blocked`,
    /// keeps the layer dirty but unqueued (no every-frame spam), badges
    /// through `BudgetBlocked`, and retries when bytes fall.
    #[test]
    fn budget_refusal_blocks_stays_dirty_and_retries_when_bytes_fall() {
        let mut stack = LayerStack::new();
        let params = channel_params("umber.budget", vec![MaterialChannel::named("baseColor")]);
        let layer = material_layer(&mut stack, "B", params, 1.0);
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.budget".into(),
            solid_def([1.0, 0.0, 0.0], "baseColor"),
        );
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        io.refuse_publish = Some(9_999_999);
        let layers = std::slice::from_ref(&layer);
        let (mut states, reports) = drive(layers, &definitions, &mut io);
        assert!(
            matches!(
                &reports[0].result,
                MaterialResult::BudgetBlocked { bytes: 9_999_999 }
            ),
            "got {:?}",
            reports[0].result
        );
        let state = states.state(layer.id).expect("state tracked");
        assert_eq!(state.budget_blocked(), Some(9_999_999));
        assert!(state.is_dirty(), "the layer stays dirty");
        assert!(
            states.dirty_ids().is_empty(),
            "but leaves the queue (no every-frame spam)"
        );
        // Bytes fall (a plane freed): the retry requeues, and the publish
        // lands on the next drain.
        io.refuse_publish = None;
        states.retry_budget_freed(1_000);
        assert!(states.dirty_ids().contains(&layer.id));
        let reports = refresh_dirty_materials(
            &mut states,
            layers,
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert!(
            matches!(&reports[0].result, MaterialResult::Refreshed { .. }),
            "got {:?}",
            reports[0].result
        );
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);
        assert_eq!(states.state(layer.id).unwrap().budget_blocked(), None);
    }
}
