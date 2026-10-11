//! The layer stack: paint/fill/folder layers with blend modes, opacity, and
//! masks (requirements.md §2).
//!
//! [`LayerStack`] is a flat, ordered `Vec<Layer>`. Folder *membership* (which
//! layers nest inside a given folder) is not modeled yet — see
//! LANDING_NOTES.md for why that is an open question left to the Wave 2
//! compositor rather than guessed at here.

use serde::{Deserialize, Serialize};

use std::mem::size_of;

use umber_mesh::FillParams;

use crate::undo::Command;

/// What a layer contains and how its children (if any) relate to the stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayerKind {
    /// A layer painted into directly with brush strokes.
    Paint,
    /// A layer filled procedurally via a projection (UV/tri-planar/etc).
    /// Stays a plain tag: the evaluatable parameters live on
    /// [`Layer::fill`] (`Some` exactly for fill layers), so the
    /// journal/project matches over `LayerKind` never churn when the fill
    /// model grows.
    Fill,
    /// A group of layers.
    ///
    /// `passthrough: true` means child layers blend directly with the
    /// layers below the folder, as if the folder were not there. `false`
    /// means the folder's contents are flattened to a single result first,
    /// and that result blends with layers below using the folder layer's
    /// own `blend_mode`/`opacity`. See [`BlendMode::Passthrough`] for how
    /// this flag relates to the blend-mode enum.
    Folder {
        /// See the variant's own doc comment above.
        passthrough: bool,
    },
    /// An instance: renders its source layer's composited pixels.
    ///
    /// Source-edits propagate: the composite eval walks instances
    /// transitively (A instances B instances C → eval order C,B,A — see
    /// [`LayerStack::instance_eval_order`]). The instance graph is a DAG:
    /// a would-be cycle is rejected at attach time with
    /// [`InstanceError::Cycle`].
    ///
    /// The `Layer`'s own `opacity`/`blend_mode`/`mask` act as per-instance
    /// OVERLAYS on the link (not edits to the source pixels): an instance
    /// can restyle its source without touching it. Instances carry no
    /// pixels of their own, so painting one directly is rejected with
    /// [`InstanceError::PaintOnInstance`].
    Instance {
        /// The instanced layer's id (same texture set).
        source_id: u64,
    },
}

/// Everything that can go wrong with layer instancing (§3:41).
///
/// JUDGMENT CALL — cycle logic lives here as a small DFS rather than
/// reusing `umber-graph`'s `topo.rs`: `umber-core` is the dependency-minimal
/// document model (it does not depend on `umber-graph`), and pulling the
/// graph crate in for one DFS would invert the layering. The algorithm is
/// the same shape (Kahn sweep for ordering, DFS for the would-be-cycle
/// probe) and is documented as a structural mirror.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum InstanceError {
    /// No layer with this id exists in the stack.
    #[error("unknown layer {0}")]
    UnknownLayer(u64),
    /// The requested source layer does not exist.
    #[error("instance {instance} references unknown source {source}")]
    UnknownSource {
        /// The instance layer's id.
        instance: u64,
        /// The missing source id.
        source: u64,
    },
    /// The layer is not an instance (retarget/overlay ops need one).
    #[error("layer {0} is not an instance")]
    NotAnInstance(u64),
    /// Attaching this source would close a cycle (participants listed,
    /// starting at the instance being retargeted).
    #[error("instance cycle detected among layers {0:?}")]
    Cycle(Vec<u64>),
    /// Instances carry no pixels: paint the source instead.
    #[error("layer {instance} is an instance of {source}: paint the source, not the instance")]
    PaintOnInstance {
        /// The instance a paint was attempted on.
        instance: u64,
        /// The source to paint instead.
        source: u64,
    },
}

/// The core-12 blend modes plus the separable extras the stack already
/// carried (requirements.md §2: "P0 core 12, full set 4").
///
/// Blending is computed in linear space, per channel (architecture.md), via
/// [`BlendMode::blend_channel`]. A layer's `blend_mode` is meaningful for
/// `Paint`/`Fill` layers and for a `Folder` layer's own composite step;
/// `Passthrough` only makes sense on a `Folder` layer and is the
/// blend-mode-enum spelling of what [`LayerKind::Folder`]'s `passthrough`
/// flag already controls. The two are kept as separate fields (flag vs.
/// enum variant) because the task's authoritative source for "does this
/// folder pass through" is the flag; the compositor (not built in this
/// task) is expected to treat a folder's `blend_mode: Passthrough` as
/// redundant with `passthrough: true` and should not need to special-case
/// a mismatch, but resolving that precisely is left open for Wave 2's
/// compositor work.
///
/// # Blend-set census (2026-10-10)
///
/// Task core-12 was: normal, passthrough, multiply, screen, overlay,
/// soft-light, hard-light, color-dodge, color-burn, linear-dodge,
/// linear-burn, add. Found in the enum before this task: Normal,
/// Passthrough, Multiply, Screen, Overlay, SoftLight, HardLight, Add
/// (8 of 12 — `Add` *is* Painter's "Linear Dodge (Add)", vocabulary item
/// 10 in docs/research/01-painter-feature-inventory.md §2, so it covers
/// both the `linear-dodge` and the `add` slots). Missing and added here:
/// `ColorDodge`, `ColorBurn`, `LinearBurn` (see each variant's docs for
/// the math and the divide-by-zero guards).
///
/// Already present beyond the task's core-12, kept as-is (all four are
/// separable members of Painter's full-32 list, so they stay valid):
/// `Darken` (Min), `Lighten` (Max), `Subtract`, `Difference`.
///
/// P0-full remainder — deliberately NOT added (Wave 4 compositor scope;
/// adding enum variants with no consumer would be dead code, and two of
/// the three families cannot be expressed in this function's per-channel
/// scalar plumbing at all):
/// - HSV modes (Tint, Saturation, Color, Value): non-separable, need an
///   RGB-triplet fn, not a per-channel scalar.
/// - Normal-map modes (Combine/Whiteout, Detail/RNM, Inverse Detail):
///   need decoded normal vectors plus channel-aware routing (and Detail
///   is the default for the normal channel, per the inventory doc).
/// - Other separable Painter modes with no variant yet (Divide, Inverse
///   Divide, Inverse Subtract, Exclusion, Signed Addition/AddSub, Vivid
///   Light, Linear Light, Pin Light, Disable, Replace): expressible in
///   [`BlendMode::blend_channel`]'s shape when the compositor needs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlendMode {
    /// Top layer fully replaces what's below it, modulated by opacity.
    #[default]
    Normal,
    /// Folder-only: children blend directly with layers below the folder,
    /// as if the folder were not there. See [`LayerKind::Folder`].
    /// Not a pixel op: [`BlendMode::blend_channel`] returns `None`.
    Passthrough,
    /// Result × base — darkens; black stays black, white is a no-op.
    Multiply,
    /// Inverse-multiply — lightens; white stays white, black is a no-op.
    Screen,
    /// Multiply below 0.5 result luminance, Screen above — contrast boost.
    Overlay,
    /// Per-channel minimum of result and base.
    Darken,
    /// Per-channel maximum of result and base.
    Lighten,
    /// Painter's "Linear Dodge (Add)": `min(1, src + dst)` — linear-light
    /// additive blend; clamps at white. Covers both the `linear-dodge`
    /// and the `add` slots of the task's core-12 (they are one mode).
    Add,
    /// Painter's "Subtract": `max(0, dst - src)` — base minus blend;
    /// clamps at black. NOT the same as [`BlendMode::LinearBurn`].
    Subtract,
    /// Absolute difference between result and base.
    Difference,
    /// Low-contrast Overlay variant (Photoshop/Substance-style soft light,
    /// Pegtop approximation — see [`BlendMode::blend_channel`]).
    SoftLight,
    /// Higher-contrast Overlay variant (roles of result/base swapped).
    HardLight,
    /// Brightens the base toward white proportionally to the blend:
    /// `src >= 1` yields white (guarded — a naive `dst / (1 - src)`
    /// divides by zero), else `min(1, dst / (1 - src))`.
    ColorDodge,
    /// Darkens the base toward black proportionally to the blend:
    /// `src <= 0` yields black (guarded), else
    /// `1 - min(1, (1 - dst) / src)`.
    ColorBurn,
    /// Painter's "Linear Burn": `max(0, src + dst - 1)` — darkens every
    /// pair whose sum is under white; clamps at black. Distinct from
    /// [`BlendMode::Subtract`] (`dst - src`): e.g. src=0.6, dst=0.7 gives
    /// 0.3 here, 0.1 under Subtract.
    LinearBurn,
}

impl BlendMode {
    /// Clamps a linear-space channel value to `0.0..=1.0` for blending.
    ///
    /// `±inf` saturates to the nearer end; `NaN` maps to `0.0` rather than
    /// poisoning the composite (a `NaN` texel must still composite to
    /// *something* finite — black is the safe choice, matching how
    /// [`LayerStack::set_opacity`] refuses to store non-finite input
    /// instead of letting it through).
    fn clamp01(v: f32) -> f32 {
        if v.is_nan() {
            0.0
        } else {
            v.clamp(0.0, 1.0)
        }
    }

    /// Blends one linear-space channel: `src` is the top (blend) layer's
    /// value, `dst` the accumulated base below — the same src/dst
    /// convention as the stamp composite (`src * cov + dst * (1 - …)`).
    ///
    /// Returns `None` for [`BlendMode::Passthrough`], which is structural
    /// (folder children blend directly below) rather than a pixel op —
    /// callers must handle folders before reaching per-pixel math.
    /// Every other variant returns `Some` finite `0.0..=1.0` value for any
    /// finite *or* non-finite input (inputs pass through [`BlendMode`]'s
    /// private clamp first, and the dodge/burn guards below keep the
    /// `1 - src` / `src` divisors off zero).
    ///
    /// Soft-light is the Pegtop approximation
    /// `((1 - 2·src)·dst + 2·src)·dst`: exact no-op at 50% gray
    /// (`src = 0.5` returns `dst`), softer than Overlay at the extremes —
    /// the standard cheap Photoshop/Substance-style form.
    pub fn blend_channel(self, src: f32, dst: f32) -> Option<f32> {
        let (s, d) = (Self::clamp01(src), Self::clamp01(dst));
        let v = match self {
            BlendMode::Normal => s,
            BlendMode::Passthrough => return None,
            BlendMode::Multiply => s * d,
            BlendMode::Screen => s + d - s * d,
            BlendMode::Overlay => {
                if d <= 0.5 {
                    2.0 * s * d
                } else {
                    1.0 - 2.0 * (1.0 - s) * (1.0 - d)
                }
            }
            BlendMode::Darken => s.min(d),
            BlendMode::Lighten => s.max(d),
            BlendMode::Add => (s + d).min(1.0),
            BlendMode::Subtract => (d - s).max(0.0),
            BlendMode::Difference => (s - d).abs(),
            BlendMode::SoftLight => ((1.0 - 2.0 * s) * d + 2.0 * s) * d,
            BlendMode::HardLight => {
                if s <= 0.5 {
                    2.0 * s * d
                } else {
                    1.0 - 2.0 * (1.0 - s) * (1.0 - d)
                }
            }
            BlendMode::ColorDodge => {
                if s >= 1.0 {
                    1.0
                } else {
                    (d / (1.0 - s)).min(1.0)
                }
            }
            BlendMode::ColorBurn => {
                if s <= 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - d) / s).min(1.0)
                }
            }
            BlendMode::LinearBurn => (s + d - 1.0).max(0.0),
        };
        Some(Self::clamp01(v))
    }

    /// Blends one channel and mixes the result with the base according to
    /// `opacity` (`0` = base unchanged, `1` = full blend effect) — the
    /// per-pixel half of a layer's `blend_mode` + `opacity` pair.
    ///
    /// Non-finite `opacity` is treated as `0.0` (base unchanged), mirroring
    /// [`LayerStack::set_opacity`]'s refusal to store non-finite input.
    /// Returns `None` exactly when [`BlendMode::blend_channel`] does
    /// (`Passthrough`).
    pub fn composite_channel(self, src: f32, dst: f32, opacity: f32) -> Option<f32> {
        let blend = self.blend_channel(src, dst)?;
        let base = Self::clamp01(dst);
        let o = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some(Self::clamp01(base + (blend - base) * o))
    }
}

/// A paint mask attached to a layer.
///
/// Pixel data lives in the tile pool (painting core, Wave 2); this is only
/// the metadata half — name and whether the mask currently affects
/// compositing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerMask {
    /// Display name, shown in the layer-stack panel next to the layer's own.
    pub name: String,
    /// Whether the mask currently affects compositing.
    pub enabled: bool,
}

/// One entry in a [`LayerStack`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// Monotonically-allocated within the owning [`LayerStack`]; stable
    /// across save/load (see project.rs).
    pub id: u64,
    /// Display name, shown in the layer-stack panel.
    pub name: String,
    /// What this layer contains (paint/fill/folder).
    pub kind: LayerKind,
    /// Clamped to `0.0..=1.0` by every mutator on [`LayerStack`].
    pub opacity: f32,
    /// Whether this layer contributes to compositing.
    pub visible: bool,
    /// How this layer's result combines with the layers below it.
    pub blend_mode: BlendMode,
    /// Attached paint mask, if any.
    pub mask: Option<LayerMask>,
    /// The fill source + projection, `Some` exactly for [`LayerKind::Fill`]
    /// layers. `#[serde(default)]` so projects saved before the fill row
    /// landed (no such key) still load — as paint-style layers with no
    /// evaluatable fill.
    #[serde(default)]
    pub fill: Option<FillParams>,
}

impl Layer {
    /// Approximate RAM held by this layer, in bytes: the inline struct
    /// plus heap name bytes plus the fill params' snapshot bytes (if any).
    /// See [`LayerStack::memory_bytes`] for the honesty contract (metadata
    /// only, no GPU tiles yet).
    pub fn memory_bytes(&self) -> usize {
        size_of::<Layer>()
            + self.name.len()
            + self.mask.as_ref().map(|m| m.name.len()).unwrap_or(0)
            + self
                .fill
                .as_ref()
                .map(FillParams::memory_bytes)
                .unwrap_or(0)
    }
}

/// An ordered stack of layers for one texture set.
///
/// Index 0 is the bottom of the stack; the last element is the top.
/// Mutating methods return the *previous* value — an "op handle" callers
/// (notably [`LayerCommand`]) use to build undo without re-querying the
/// stack afterwards.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerStack {
    /// Stack order, bottom (index 0) to top.
    pub layers: Vec<Layer>,
    next_id: u64,
}

impl Default for LayerStack {
    fn default() -> Self {
        Self::new()
    }
}

impl LayerStack {
    /// An empty stack with id allocation starting at 0.
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            next_id: 0,
        }
    }

    /// Rebuilds a stack from layers loaded from disk plus the id counter
    /// that was in effect when they were saved.
    ///
    /// Takes the saved counter rather than deriving `max(id) + 1` so that
    /// ids stay monotonic across a save/load cycle even if the top layer
    /// was deleted before saving (see project.rs LayerSetOrder).
    pub fn from_parts(layers: Vec<Layer>, saved_next_id: u64) -> Self {
        let min_safe = layers.iter().map(|l| l.id + 1).max().unwrap_or(0);
        Self {
            layers,
            next_id: saved_next_id.max(min_safe),
        }
    }

    /// The id the next [`add_layer`](Self::add_layer) call will allocate.
    pub fn next_layer_id(&self) -> u64 {
        self.next_id
    }

    /// The current stack position of the layer with `id`, if it exists.
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    /// Looks up a layer by id.
    pub fn layer(&self, id: u64) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    /// Looks up a layer by id, mutably.
    pub fn layer_mut(&mut self, id: u64) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// Pushes a new layer onto the top of the stack, returning its
    /// newly-allocated id.
    pub fn add_layer(&mut self, name: impl Into<String>, kind: LayerKind) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.layers.push(Layer {
            id,
            name: name.into(),
            kind,
            opacity: 1.0,
            visible: true,
            blend_mode: BlendMode::Normal,
            mask: None,
            fill: None,
        });
        id
    }

    /// Re-inserts a layer at the top of the stack under a *specific* id,
    /// without allocating a fresh one.
    ///
    /// Only used by [`LayerCommand::Add`]'s redo path, so that redoing an
    /// add restores the exact same id a later command in the journal (e.g.
    /// a `SetOpacity` on that id) may still refer to. Keeps `next_id`
    /// monotonic even though `id` was not issued by this call.
    fn add_layer_with_id(&mut self, id: u64, name: String, kind: LayerKind) {
        self.layers.push(Layer {
            id,
            name,
            kind,
            opacity: 1.0,
            visible: true,
            blend_mode: BlendMode::Normal,
            mask: None,
            fill: None,
        });
        self.next_id = self.next_id.max(id + 1);
    }

    /// Removes the layer with `id`, returning its former `(index, Layer)` so
    /// it can be reinserted verbatim (see [`insert_layer`](Self::insert_layer)).
    pub fn remove_layer(&mut self, id: u64) -> Option<(usize, Layer)> {
        let index = self.index_of(id)?;
        Some((index, self.layers.remove(index)))
    }

    /// Re-inserts a previously-removed layer at `index` (clamped to the
    /// current length).
    pub fn insert_layer(&mut self, index: usize, layer: Layer) {
        self.next_id = self.next_id.max(layer.id + 1);
        let index = index.min(self.layers.len());
        self.layers.insert(index, layer);
    }

    /// Moves the layer at `from` to `to` (clamped to the valid range),
    /// returning the index it actually landed at, or `None` if `from` was
    /// out of range.
    pub fn reorder(&mut self, from: usize, to: usize) -> Option<usize> {
        if from >= self.layers.len() {
            return None;
        }
        let layer = self.layers.remove(from);
        let to = to.min(self.layers.len());
        self.layers.insert(to, layer);
        Some(to)
    }

    /// Sets opacity, clamped to `0.0..=1.0`. Non-finite input is ignored
    /// (the layer's opacity is left unchanged). Returns the previous value.
    pub fn set_opacity(&mut self, id: u64, value: f32) -> Option<f32> {
        let layer = self.layer_mut(id)?;
        let old = layer.opacity;
        if value.is_finite() {
            layer.opacity = value.clamp(0.0, 1.0);
        }
        Some(old)
    }

    /// Returns the previous visibility.
    pub fn set_visible(&mut self, id: u64, value: bool) -> Option<bool> {
        let layer = self.layer_mut(id)?;
        let old = layer.visible;
        layer.visible = value;
        Some(old)
    }

    /// Returns the previous blend mode.
    pub fn set_blend_mode(&mut self, id: u64, mode: BlendMode) -> Option<BlendMode> {
        let layer = self.layer_mut(id)?;
        let old = layer.blend_mode;
        layer.blend_mode = mode;
        Some(old)
    }

    /// Sets a layer's fill parameters (source + projection + mask), returning the
    /// previous value (`None` when the layer had no fill params — including
    /// non-fill layers, which accept the write but stay non-fill: the
    /// evaluator only runs `fill` on [`LayerKind::Fill`] layers, so a
    /// stray write is inert data, never a behavior change).
    pub fn set_fill(&mut self, id: u64, params: FillParams) -> Option<Option<FillParams>> {
        let layer = self.layer_mut(id)?;
        Some(layer.fill.replace(params))
    }

    /// Pushes a new instance layer onto the top of the stack, rendering
    /// `source_id`'s composited pixels. The new layer's own
    /// `opacity`/`blend_mode`/`mask` are its per-instance overlays.
    ///
    /// # Errors
    ///
    /// [`InstanceError::UnknownSource`] when `source_id` names no layer.
    /// A fresh instance cannot cycle (its id is new to the graph), so no
    /// [`InstanceError::Cycle`] arises here — cycles are rejected by
    /// [`Self::set_instance_source`].
    pub fn add_instance(
        &mut self,
        name: impl Into<String>,
        source_id: u64,
    ) -> Result<u64, InstanceError> {
        if self.layer(source_id).is_none() {
            return Err(InstanceError::UnknownSource {
                instance: self.next_id,
                source: source_id,
            });
        }
        let id = self.next_id;
        self.next_id += 1;
        self.layers.push(Layer {
            id,
            name: name.into(),
            kind: LayerKind::Instance { source_id },
            opacity: 1.0,
            visible: true,
            blend_mode: BlendMode::Normal,
            mask: None,
            fill: None,
        });
        Ok(id)
    }

    /// The source of an instance layer, if `id` names one (`None` for
    /// non-instance layers or unknown ids).
    pub fn instance_source(&self, id: u64) -> Option<u64> {
        match self.layer(id) {
            Some(Layer {
                kind: LayerKind::Instance { source_id },
                ..
            }) => Some(*source_id),
            _ => None,
        }
    }

    /// Whether `id` names an instance layer.
    pub fn is_instance(&self, id: u64) -> bool {
        self.instance_source(id).is_some()
    }

    /// Retargets an instance layer onto a new source, rejecting the
    /// would-be cycle at attach time. Returns the previous source.
    ///
    /// # Errors
    ///
    /// [`InstanceError::UnknownLayer`] when `id` names no layer;
    /// [`InstanceError::NotAnInstance`] when it names a non-instance;
    /// [`InstanceError::UnknownSource`] when `source_id` names no layer;
    /// [`InstanceError::Cycle`] when the retarget would close a cycle
    /// (direct `A→B→A` or indirect — the DFS walks the transitive chain).
    pub fn set_instance_source(&mut self, id: u64, source_id: u64) -> Result<u64, InstanceError> {
        let layer = self.layer(id).ok_or(InstanceError::UnknownLayer(id))?;
        let old = match layer {
            Layer {
                kind: LayerKind::Instance { source_id },
                ..
            } => *source_id,
            _ => return Err(InstanceError::NotAnInstance(id)),
        };
        if self.layer(source_id).is_none() {
            return Err(InstanceError::UnknownSource {
                instance: id,
                source: source_id,
            });
        }
        if let Some(participants) = self.would_cycle(id, source_id) {
            return Err(InstanceError::Cycle(participants));
        }
        if let Some(layer) = self.layer_mut(id) {
            layer.kind = LayerKind::Instance { source_id };
        }
        Ok(old)
    }

    /// Whether pointing `instance_id` at `new_source` would close a cycle:
    /// `Some(participants)` (starting at `instance_id`) when yes, `None`
    /// when safe. A small iterative DFS over the instance edges — the
    /// structural mirror of `umber-graph`'s Kahn check (see
    /// [`InstanceError`] for why it lives here).
    fn would_cycle(&self, instance_id: u64, new_source: u64) -> Option<Vec<u64>> {
        // Follow source links transitively from the candidate source: if
        // the walk reaches the instance being retargeted, the edge closes
        // a cycle. Non-instance layers terminate the walk (they have no
        // outgoing edge).
        let mut chain = vec![instance_id];
        let mut cursor = new_source;
        // Bounded by the layer count: a longer walk must repeat a layer,
        // which is itself a cycle through the new edge.
        for _ in 0..=self.layers.len() {
            if cursor == instance_id {
                chain.push(cursor);
                return Some(chain);
            }
            chain.push(cursor);
            match self.layer(cursor) {
                Some(Layer {
                    kind: LayerKind::Instance { source_id },
                    ..
                }) => cursor = *source_id,
                _ => return None,
            }
        }
        chain.push(cursor);
        Some(chain)
    }

    /// The transitive source chain of `id`, nearest first
    /// (`A→B→C` yields `[B, C]`). Stops at the first non-instance layer
    /// or missing link; empty for non-instances.
    pub fn instance_chain(&self, id: u64) -> Vec<u64> {
        let mut chain = Vec::new();
        let mut cursor = match self.instance_source(id) {
            Some(s) => s,
            None => return chain,
        };
        for _ in 0..=self.layers.len() {
            chain.push(cursor);
            match self.layer(cursor) {
                Some(Layer {
                    kind: LayerKind::Instance { source_id },
                    ..
                }) => cursor = *source_id,
                _ => break,
            }
        }
        chain
    }

    /// Evaluation order for the instance subgraph: sources before their
    /// instances, transitively (A instances B instances C → `C` before
    /// `B` before `A`). Non-instance layers keep stack order; instance
    /// layers sort after their transitive sources (stable otherwise).
    ///
    /// # Errors
    ///
    /// [`InstanceError::Cycle`] if the stored graph already cycles
    /// (attach-time rejection in [`Self::set_instance_source`] should make
    /// this unreachable — it fires for hand-built or legacy files), or
    /// [`InstanceError::UnknownSource`] on a dangling source link.
    pub fn instance_eval_order(&self) -> Result<Vec<u64>, InstanceError> {
        // Validate links first (dangling sources are loud, not skipped).
        for layer in &self.layers {
            if let LayerKind::Instance { source_id } = &layer.kind {
                if self.layer(*source_id).is_none() {
                    return Err(InstanceError::UnknownSource {
                        instance: layer.id,
                        source: *source_id,
                    });
                }
            }
        }
        // Kahn sweep over instance edges (source → instance), restricted
        // to instance layers; non-instances emit in stack order first.
        use std::collections::HashMap;
        let instances: Vec<u64> = self
            .layers
            .iter()
            .filter(|l| matches!(l.kind, LayerKind::Instance { .. }))
            .map(|l| l.id)
            .collect();
        let in_set: std::collections::HashSet<u64> = instances.iter().copied().collect();
        let mut indegree: HashMap<u64, usize> = instances.iter().map(|&id| (id, 0)).collect();
        let mut outs: HashMap<u64, Vec<u64>> = HashMap::new();
        for &id in &instances {
            let src = self.instance_source(id).expect("instance checked");
            if in_set.contains(&src) {
                *indegree.get_mut(&id).expect("instance keyed") += 1;
                outs.entry(src).or_default().push(id);
            }
        }
        let mut queue: Vec<u64> = indegree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();
        queue.sort_unstable();
        let mut order = Vec::with_capacity(instances.len());
        while let Some(&id) = queue.first() {
            queue.remove(0);
            order.push(id);
            if let Some(next) = outs.get(&id) {
                let mut next = next.clone();
                next.sort_unstable();
                for to in next {
                    let d = indegree.get_mut(&to).expect("instance keyed");
                    *d -= 1;
                    if *d == 0 {
                        queue.push(to);
                        queue.sort_unstable();
                    }
                }
            }
        }
        if order.len() != instances.len() {
            let mut cyclic: Vec<u64> = indegree
                .iter()
                .filter(|(_, &d)| d > 0)
                .map(|(&id, _)| id)
                .collect();
            cyclic.sort_unstable();
            return Err(InstanceError::Cycle(cyclic));
        }
        // Full stack order: plain layers in stack order, then instances in
        // dependency order (sources — plain or ordered instances — always
        // precede their instances).
        let mut full: Vec<u64> = self
            .layers
            .iter()
            .filter(|l| !matches!(l.kind, LayerKind::Instance { .. }))
            .map(|l| l.id)
            .collect();
        full.extend(order);
        Ok(full)
    }

    /// Rejects direct painting on an instance layer: instances carry no
    /// pixels, so a paint attempt is a typed error naming the source to
    /// paint instead. `Ok(())` for paintable (non-instance) layers.
    ///
    /// # Errors
    ///
    /// [`InstanceError::UnknownLayer`] for unknown ids;
    /// [`InstanceError::PaintOnInstance`] for instances (names the source).
    pub fn check_paintable(&self, id: u64) -> Result<(), InstanceError> {
        let layer = self.layer(id).ok_or(InstanceError::UnknownLayer(id))?;
        match &layer.kind {
            LayerKind::Instance { source_id } => Err(InstanceError::PaintOnInstance {
                instance: id,
                source: *source_id,
            }),
            _ => Ok(()),
        }
    }

    /// Approximate RAM held by the whole stack, in bytes: per-layer
    /// [`Layer::memory_bytes`] plus the `Vec` buffer itself.
    ///
    /// An honest approximation, not a budget: layer metadata only (names,
    /// struct fields). GPU-tile bytes are not modeled here — they arrive
    /// with the tile-pool slice, at which point this sum grows a tile term.
    /// The undo soak test samples this after each push to assert the growth
    /// curve's shape (sub-linear) plus an empirical byte ceiling.
    pub fn memory_bytes(&self) -> usize {
        self.layers.iter().map(Layer::memory_bytes).sum::<usize>()
            + self.layers.capacity() * size_of::<Layer>()
    }
}

/// A reversible [`LayerStack`] edit, for use with `UndoStack<LayerCommand>`.
///
/// One enum rather than a family of small command structs, because
/// [`crate::undo::UndoStack`] is monomorphic over a single command type —
/// callers need one type that covers every op to run a single journal over
/// a layer stack.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerCommand {
    /// Adds a new layer to the top of the stack.
    Add {
        /// Name of the layer to create.
        name: String,
        /// Kind of the layer to create.
        kind: LayerKind,
        /// `None` until the first `apply`; afterwards holds the allocated
        /// id so redo can restore the exact same one.
        id: Option<u64>,
    },
    /// Removes an existing layer by id.
    Remove {
        /// Id of the layer to remove.
        id: u64,
        /// Captured `(index, Layer)` after `apply`, for `revert`.
        removed: Option<(usize, Layer)>,
    },
    /// Moves a layer from one stack position to another.
    Reorder {
        /// Source index.
        from: usize,
        /// Requested destination index (clamped on apply).
        to: usize,
        /// The index the layer actually landed at after `apply`.
        applied_to: Option<usize>,
    },
    /// Sets a layer's opacity.
    SetOpacity {
        /// Id of the layer to update.
        id: u64,
        /// New opacity value.
        new: f32,
        /// Previous opacity, captured by `apply` for `revert`.
        old: Option<f32>,
    },
    /// Sets a layer's visibility.
    SetVisible {
        /// Id of the layer to update.
        id: u64,
        /// New visibility value.
        new: bool,
        /// Previous visibility, captured by `apply` for `revert`.
        old: Option<bool>,
    },
    /// Sets a layer's blend mode.
    SetBlendMode {
        /// Id of the layer to update.
        id: u64,
        /// New blend mode.
        new: BlendMode,
        /// Previous blend mode, captured by `apply` for `revert`.
        old: Option<BlendMode>,
    },
    /// Sets a layer's fill parameters (source + projection + mask).
    SetFill {
        /// Id of the layer to update.
        id: u64,
        /// New fill parameters.
        new: FillParams,
        /// Previous fill parameters (`None` when the layer had none),
        /// captured by `apply` for `revert`.
        old: Option<Option<FillParams>>,
    },
    /// Retargets an instance layer onto a new source (§3:41). Cycle-safe:
    /// the apply reuses [`LayerStack::set_instance_source`], so a redo
    /// that would now close a cycle is ignored rather than applied.
    SetInstanceSource {
        /// Id of the instance layer to retarget.
        id: u64,
        /// New source layer id.
        new: u64,
        /// Previous source, captured by `apply` for `revert`.
        old: Option<u64>,
    },
}

impl LayerCommand {
    /// Builds an [`Add`](LayerCommand::Add) command for a new layer.
    pub fn add(name: impl Into<String>, kind: LayerKind) -> Self {
        LayerCommand::Add {
            name: name.into(),
            kind,
            id: None,
        }
    }

    /// Builds a [`Remove`](LayerCommand::Remove) command for an existing layer.
    pub fn remove(id: u64) -> Self {
        LayerCommand::Remove { id, removed: None }
    }

    /// Builds a [`Reorder`](LayerCommand::Reorder) command.
    pub fn reorder(from: usize, to: usize) -> Self {
        LayerCommand::Reorder {
            from,
            to,
            applied_to: None,
        }
    }

    /// Builds a [`SetOpacity`](LayerCommand::SetOpacity) command.
    pub fn set_opacity(id: u64, value: f32) -> Self {
        LayerCommand::SetOpacity {
            id,
            new: value,
            old: None,
        }
    }

    /// Builds a [`SetVisible`](LayerCommand::SetVisible) command.
    pub fn set_visible(id: u64, value: bool) -> Self {
        LayerCommand::SetVisible {
            id,
            new: value,
            old: None,
        }
    }

    /// Builds a [`SetBlendMode`](LayerCommand::SetBlendMode) command.
    pub fn set_blend_mode(id: u64, mode: BlendMode) -> Self {
        LayerCommand::SetBlendMode {
            id,
            new: mode,
            old: None,
        }
    }

    /// Builds a [`SetFill`](LayerCommand::SetFill) command.
    pub fn set_fill(id: u64, params: FillParams) -> Self {
        LayerCommand::SetFill {
            id,
            new: params,
            old: None,
        }
    }

    /// Builds a [`SetInstanceSource`](LayerCommand::SetInstanceSource) command.
    pub fn set_instance_source(id: u64, source: u64) -> Self {
        LayerCommand::SetInstanceSource {
            id,
            new: source,
            old: None,
        }
    }
}

impl Command for LayerCommand {
    type Doc = LayerStack;

    fn apply(&mut self, doc: &mut LayerStack) {
        match self {
            LayerCommand::Add { name, kind, id } => match id {
                None => *id = Some(doc.add_layer(name.clone(), kind.clone())),
                Some(existing) => doc.add_layer_with_id(*existing, name.clone(), kind.clone()),
            },
            LayerCommand::Remove { id, removed } => *removed = doc.remove_layer(*id),
            LayerCommand::Reorder {
                from,
                to,
                applied_to,
            } => {
                *applied_to = doc.reorder(*from, *to);
            }
            LayerCommand::SetOpacity { id, new, old } => *old = doc.set_opacity(*id, *new),
            LayerCommand::SetVisible { id, new, old } => *old = doc.set_visible(*id, *new),
            LayerCommand::SetBlendMode { id, new, old } => *old = doc.set_blend_mode(*id, *new),
            LayerCommand::SetFill { id, new, old } => *old = doc.set_fill(*id, new.clone()),
            LayerCommand::SetInstanceSource { id, new, old } => {
                *old = doc.set_instance_source(*id, *new).ok();
            }
        }
    }

    fn revert(&mut self, doc: &mut LayerStack) {
        match self {
            // Deliberately does not take() the id: redo re-applies Add via
            // the Some(existing) arm above, which needs it to restore the
            // same id rather than allocating a new one.
            LayerCommand::Add { id, .. } => {
                if let Some(id) = id {
                    doc.remove_layer(*id);
                }
            }
            LayerCommand::Remove { removed, .. } => {
                if let Some((index, layer)) = removed.take() {
                    doc.insert_layer(index, layer);
                }
            }
            LayerCommand::Reorder {
                from, applied_to, ..
            } => {
                if let Some(to) = applied_to.take() {
                    doc.reorder(to, *from);
                }
            }
            LayerCommand::SetOpacity { id, old, .. } => {
                if let Some(value) = old.take() {
                    doc.set_opacity(*id, value);
                }
            }
            LayerCommand::SetVisible { id, old, .. } => {
                if let Some(value) = old.take() {
                    doc.set_visible(*id, value);
                }
            }
            LayerCommand::SetBlendMode { id, old, .. } => {
                if let Some(value) = old.take() {
                    doc.set_blend_mode(*id, value);
                }
            }
            LayerCommand::SetFill { id, old, .. } => {
                if let Some(previous) = old.take() {
                    match previous {
                        Some(params) => {
                            doc.set_fill(*id, params);
                        }
                        // The layer had no fill params before: undo removes
                        // what the apply set (there is no "clear fill"
                        // mutator — `None`-before means absent-before).
                        None => {
                            if let Some(layer) = doc.layer_mut(*id) {
                                layer.fill = None;
                            }
                        }
                    }
                }
            }
            LayerCommand::SetInstanceSource { id, old, .. } => {
                if let Some(previous) = old.take() {
                    // Undo of a retarget restores the old source; like the
                    // apply, it goes through the cycle guard (a redo/undo
                    // pair that now cycles is ignored, never forced).
                    let _ = doc.set_instance_source(*id, previous);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::undo::UndoStack;

    fn ids(stack: &LayerStack) -> Vec<u64> {
        stack.layers.iter().map(|l| l.id).collect()
    }

    #[test]
    fn add_and_remove_round_trip_index_and_id() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("Base", LayerKind::Paint);
        let b = stack.add_layer("Detail", LayerKind::Paint);
        assert_eq!(stack.layers.len(), 2);
        assert_eq!(stack.layers[0].id, a);
        assert_eq!(stack.layers[1].id, b);

        let (index, removed) = stack.remove_layer(a).expect("layer a exists");
        assert_eq!(index, 0);
        assert_eq!(removed.id, a);
        assert_eq!(stack.layers.len(), 1);

        stack.insert_layer(index, removed);
        assert_eq!(stack.layers[0].id, a);
    }

    #[test]
    fn reorder_moves_layer_and_clamps_target() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);
        let b = stack.add_layer("B", LayerKind::Paint);
        let c = stack.add_layer("C", LayerKind::Paint);
        assert_eq!(ids(&stack), vec![a, b, c]);

        let landed = stack.reorder(0, 2).unwrap();
        assert_eq!(landed, 2);
        assert_eq!(ids(&stack), vec![b, c, a]);

        let landed = stack.reorder(0, 999).unwrap();
        assert_eq!(landed, 2); // clamped to the end
        assert_eq!(ids(&stack), vec![c, a, b]);

        assert_eq!(stack.reorder(99, 0), None); // out-of-range source
    }

    #[test]
    fn set_opacity_clamps_and_ignores_non_finite() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);

        assert_eq!(stack.set_opacity(a, 0.5), Some(1.0));
        assert_eq!(stack.layer(a).unwrap().opacity, 0.5);

        stack.set_opacity(a, 5.0);
        assert_eq!(stack.layer(a).unwrap().opacity, 1.0);

        stack.set_opacity(a, -5.0);
        assert_eq!(stack.layer(a).unwrap().opacity, 0.0);

        stack.set_opacity(a, 0.75);
        stack.set_opacity(a, f32::NAN);
        assert_eq!(stack.layer(a).unwrap().opacity, 0.75); // NaN ignored, not stored
    }

    #[test]
    fn layer_command_interleaved_undo_redo_matches_direct_mutation() {
        let mut stack = LayerStack::new();
        let mut undo: UndoStack<LayerCommand> = UndoStack::new(100);

        let snapshot_empty = stack.layers.clone();

        undo.push(LayerCommand::add("Base", LayerKind::Paint), &mut stack);
        let id = stack.layers[0].id;
        let snapshot_after_add = stack.layers.clone();

        undo.push(LayerCommand::set_opacity(id, 0.5), &mut stack);
        let snapshot_after_opacity = stack.layers.clone();
        assert_eq!(stack.layer(id).unwrap().opacity, 0.5);

        undo.push(LayerCommand::set_visible(id, false), &mut stack);
        let snapshot_after_visible = stack.layers.clone();
        assert!(!stack.layer(id).unwrap().visible);

        // Undo back through each intermediate state.
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layers, snapshot_after_opacity);
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layers, snapshot_after_add);
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layers, snapshot_empty);
        assert!(!undo.undo(&mut stack));

        // Redo forward through each intermediate state, re-using the same id.
        assert!(undo.redo(&mut stack));
        assert_eq!(stack.layers, snapshot_after_add);
        assert_eq!(stack.layers[0].id, id);

        assert!(undo.redo(&mut stack));
        assert_eq!(stack.layers, snapshot_after_opacity);

        assert!(undo.redo(&mut stack));
        assert_eq!(stack.layers, snapshot_after_visible);
        assert!(!undo.can_redo());
    }

    #[test]
    fn remove_command_undo_restores_layer_at_original_index() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);
        let b = stack.add_layer("B", LayerKind::Paint);
        let mut undo: UndoStack<LayerCommand> = UndoStack::new(100);

        let before = stack.layers.clone();
        undo.push(LayerCommand::remove(a), &mut stack);
        assert_eq!(ids(&stack), vec![b]);

        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layers, before);
    }

    #[test]
    fn set_fill_round_trips_through_undo_and_json() {
        use umber_mesh::{FillProjection, FillSource};
        let mut stack = LayerStack::new();
        let id = stack.add_layer("Tint", LayerKind::Fill);
        assert_eq!(stack.layer(id).unwrap().fill, None);

        let red = FillParams {
            source: FillSource::Solid {
                color: [1.0, 0.0, 0.0, 1.0],
            },
            projection: FillProjection::Uv,
            mask: None,
        };
        let mut undo: UndoStack<LayerCommand> = UndoStack::new(100);
        undo.push(LayerCommand::set_fill(id, red.clone()), &mut stack);
        assert_eq!(stack.layer(id).unwrap().fill, Some(red.clone()));

        let planar = FillParams {
            source: red.source.clone(),
            projection: FillProjection::Planar {
                axis: umber_mesh::ProjAxis::X,
            },
            mask: None,
        };
        undo.push(LayerCommand::set_fill(id, planar.clone()), &mut stack);
        assert_eq!(stack.layer(id).unwrap().fill, Some(planar.clone()));

        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layer(id).unwrap().fill, Some(red.clone()));
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layer(id).unwrap().fill, None);
        assert!(undo.redo(&mut stack));
        assert_eq!(stack.layer(id).unwrap().fill, Some(red.clone()));

        // JSON round-trip preserves the params …
        let json = serde_json::to_string(stack.layer(id).unwrap()).unwrap();
        let back: Layer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.fill, Some(red.clone()));
        // … and pre-fill saves (no `fill` key) still load, as fill-less.
        let legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut legacy = legacy.as_object().unwrap().clone();
        legacy.remove("fill");
        let back: Layer = serde_json::from_value(serde_json::Value::Object(legacy)).unwrap();
        assert_eq!(back.fill, None);
        assert_eq!(back.name, "Tint");
        // … and pre-polygon saves (a `fill` without `mask`) still load,
        // as unmasked fills.
        let mut unmasked = serde_json::to_value(stack.layer(id).unwrap().fill.clone()).unwrap();
        unmasked.as_object_mut().unwrap().remove("mask");
        let back: FillParams = serde_json::from_value(unmasked).unwrap();
        assert_eq!(back.mask, None);
        assert_eq!(back, red);
    }

    fn blend(mode: BlendMode, src: f32, dst: f32) -> f32 {
        mode.blend_channel(src, dst)
            .unwrap_or_else(|| panic!("{mode:?} should be a separable pixel op"))
    }

    #[test]
    fn color_dodge_pins_known_value_and_guards_white_source() {
        // Known value: base 0.3 dodged by 0.5 -> 0.3 / 0.5 = 0.6.
        let v = blend(BlendMode::ColorDodge, 0.5, 0.3);
        assert!(
            (v - 0.6).abs() < 1e-6,
            "color dodge src=0.5 dst=0.3 should be 0.6, got {v}"
        );
        // Can-fail: a naive `dst / (1 - src)` divides by zero here
        // (0/0 = NaN, which the output clamp maps to 0). White blend
        // must dodge to white regardless of the base.
        assert_eq!(
            blend(BlendMode::ColorDodge, 1.0, 0.0),
            1.0,
            "color dodge by white over black must be white, not NaN-black"
        );
        assert_eq!(
            blend(BlendMode::ColorDodge, 1.0, 0.4),
            1.0,
            "color dodge by white must saturate to white"
        );
    }

    #[test]
    fn color_burn_pins_known_value_and_guards_black_source() {
        // Known value: base 0.7 burned by 0.5 -> 1 - 0.3/0.5 = 0.4.
        let v = blend(BlendMode::ColorBurn, 0.5, 0.7);
        assert!(
            (v - 0.4).abs() < 1e-6,
            "color burn src=0.5 dst=0.7 should be 0.4, got {v}"
        );
        // Can-fail: a naive `1 - (1 - dst) / src` divides by zero here.
        // Black blend must burn to black regardless of the base.
        assert_eq!(
            blend(BlendMode::ColorBurn, 0.0, 1.0),
            0.0,
            "color burn by black over white must be black"
        );
        assert_eq!(
            blend(BlendMode::ColorBurn, 0.0, 0.8),
            0.0,
            "color burn by black must stay black"
        );
    }

    #[test]
    fn linear_burn_clamps_and_differs_from_subtract() {
        // Can-fail: LinearBurn is `src + dst - 1`, NOT Subtract's
        // `dst - src`. src=0.6, dst=0.7 -> 0.3 here, 0.1 under Subtract.
        let v = blend(BlendMode::LinearBurn, 0.6, 0.7);
        assert!(
            (v - 0.3).abs() < 1e-6,
            "linear burn src=0.6 dst=0.7 should be 0.3 (not Subtract's 0.1), got {v}"
        );
        // Can-fail: the sum-minus-one goes negative and must clamp at
        // black rather than leaking a negative channel value.
        assert_eq!(
            blend(BlendMode::LinearBurn, 0.1, 0.2),
            0.0,
            "linear burn 0.1/0.2 sums under 1 and must clamp to black"
        );
        assert_eq!(
            blend(BlendMode::LinearBurn, 1.0, 1.0),
            1.0,
            "linear burn white/white stays white"
        );
    }

    #[test]
    fn add_is_linear_dodge_census_pin() {
        // Painter's vocabulary item 10 is literally "Linear Dodge (Add)":
        // one mode, two names. Pin `Add` to `min(1, src + dst)` so the
        // core-12's `linear-dodge` and `add` slots stay covered by this
        // single variant rather than a duplicate alias variant.
        for (s, d) in [(0.0, 0.0), (0.3, 0.4), (0.6, 0.7), (1.0, 1.0)] {
            assert_eq!(
                blend(BlendMode::Add, s, d),
                (s + d).min(1.0),
                "Add({s}, {d}) must equal linear-dodge min(1, src + dst)"
            );
        }
    }

    #[test]
    fn blend_census_every_separable_mode_returns_finite_clamped_math() {
        // The static census pin: every variant except Passthrough must be
        // a total pixel op — Some finite 0..=1 value for corner and
        // mid-range inputs alike. The `len` assert below is the tripwire:
        // add a variant without extending this list and it fails, forcing
        // the census (and the enum docs above) to be updated with the enum.
        let separable = [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::Add,
            BlendMode::Subtract,
            BlendMode::Difference,
            BlendMode::SoftLight,
            BlendMode::HardLight,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::LinearBurn,
        ];
        assert_eq!(separable.len(), 14, "census list must track the enum");
        for mode in separable {
            for (s, d) in [
                (0.0, 0.0),
                (1.0, 1.0),
                (1.0, 0.0),
                (0.0, 1.0),
                (0.25, 0.75),
                (0.75, 0.25),
            ] {
                let v = mode
                    .blend_channel(s, d)
                    .unwrap_or_else(|| panic!("{mode:?} must return Some"));
                assert!(
                    v.is_finite() && (0.0..=1.0).contains(&v),
                    "{mode:?}({s}, {d}) must be finite 0..=1, got {v}"
                );
            }
        }
        assert_eq!(
            BlendMode::Passthrough.blend_channel(0.5, 0.5),
            None,
            "Passthrough is structural, not a pixel op"
        );
    }

    #[test]
    fn composite_channel_honors_opacity_and_ignores_non_finite() {
        // Opacity 1 = full blend effect, 0 = base untouched.
        assert_eq!(
            BlendMode::Multiply
                .composite_channel(0.5, 0.5, 1.0)
                .expect("separable op"),
            0.25,
            "full-opacity multiply 0.5/0.5 is 0.25"
        );
        assert_eq!(
            BlendMode::Multiply
                .composite_channel(0.5, 0.5, 0.0)
                .expect("separable op"),
            0.5,
            "zero opacity must leave the base untouched"
        );
        // Half opacity mixes halfway: 0.5 + (0.25 - 0.5) * 0.5 = 0.375.
        let v = BlendMode::Multiply
            .composite_channel(0.5, 0.5, 0.5)
            .expect("separable op");
        assert!(
            (v - 0.375).abs() < 1e-6,
            "half-opacity multiply should be 0.375, got {v}"
        );
        // Non-finite opacity leaves the base unchanged, mirroring
        // `set_opacity`'s refusal to store non-finite input.
        assert_eq!(
            BlendMode::Screen
                .composite_channel(0.9, 0.2, f32::NAN)
                .expect("separable op"),
            0.2,
            "NaN opacity must leave the base untouched"
        );
        assert_eq!(
            BlendMode::Passthrough.composite_channel(0.9, 0.2, 1.0),
            None,
            "Passthrough has no composite math either"
        );
    }

    #[test]
    fn instance_renders_source_and_overlays_are_link_local() {
        // An instance renders its source's pixels: the instance holds no
        // fill of its own, while its opacity/blend/mask overlay the link.
        let mut stack = LayerStack::new();
        let src = stack.add_layer("Base", LayerKind::Paint);
        let inst = stack.add_instance("Base inst", src).expect("source exists");
        assert_eq!(stack.instance_source(inst), Some(src));
        assert!(stack.is_instance(inst));
        assert!(!stack.is_instance(src));
        // Per-instance overlays live on the link, not the pixels.
        stack.set_opacity(inst, 0.5);
        stack.set_blend_mode(inst, BlendMode::Multiply);
        assert_eq!(stack.layer(inst).unwrap().opacity, 0.5);
        assert_eq!(stack.layer(inst).unwrap().blend_mode, BlendMode::Multiply);
        // … and the source is untouched by the overlay.
        assert_eq!(stack.layer(src).unwrap().opacity, 1.0);
        assert_eq!(stack.layer(src).unwrap().blend_mode, BlendMode::Normal);
    }

    #[test]
    fn instance_direct_cycle_is_rejected_at_attach_time() {
        // A→B→A: retargeting B onto A (which instances B) must fail.
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);
        let b = stack.add_instance("B inst A", a).expect("source exists");
        assert_eq!(
            stack.set_instance_source(b, b),
            Err(InstanceError::Cycle(vec![b, b])),
            "self-instancing closes a 1-cycle"
        );
        // Build the reverse link attempt: A is paint, so make A an
        // instance of B first is impossible — instead add C instancing B,
        // then retarget B onto C (B→C→B).
        let c = stack.add_instance("C inst B", b).expect("source exists");
        match stack.set_instance_source(b, c) {
            Err(InstanceError::Cycle(participants)) => {
                assert_eq!(participants.first(), Some(&b));
                assert!(participants.contains(&c));
            }
            other => panic!("A→B→A must be rejected, got {other:?}"),
        }
        // The failed retarget changed nothing.
        assert_eq!(stack.instance_source(b), Some(a));
        let _ = a;
    }

    #[test]
    fn instance_four_hop_indirect_cycle_is_rejected() {
        // D→C→B→A chain, then retarget A (as instance) onto D: the DFS
        // must walk all four hops and reject.
        let mut stack = LayerStack::new();
        let a0 = stack.add_layer("A", LayerKind::Paint);
        let b = stack.add_instance("B inst A", a0).expect("source exists");
        let c = stack.add_instance("C inst B", b).expect("source exists");
        let d = stack.add_instance("D inst C", c).expect("source exists");
        // Turn the paint root into an instance of D would close
        // A→D→C→B→A — but A is paint, so instead retarget B onto D:
        // B→D→C→B (3 hops through the new edge).
        match stack.set_instance_source(b, d) {
            Err(InstanceError::Cycle(participants)) => {
                assert_eq!(participants.first(), Some(&b));
                for id in [b, c, d] {
                    assert!(participants.contains(&id), "participants: {participants:?}");
                }
            }
            other => panic!("4-hop indirect cycle must be rejected, got {other:?}"),
        }
        assert_eq!(stack.instance_source(b), Some(a0));
    }

    #[test]
    fn instance_eval_order_walks_transitively() {
        // A instances B instances C (paint root): eval order C,B,A —
        // sources before instances, transitively, regardless of stack
        // (insertion) order.
        let mut stack = LayerStack::new();
        let c = stack.add_layer("C", LayerKind::Paint);
        let b = stack.add_instance("B inst C", c).expect("source exists");
        let a = stack.add_instance("A inst B", b).expect("source exists");
        assert_eq!(stack.instance_chain(a), vec![b, c]);
        assert_eq!(stack.instance_chain(b), vec![c]);
        assert!(stack.instance_chain(c).is_empty());
        let order = stack.instance_eval_order().expect("acyclic");
        let pos = |id: u64| order.iter().position(|&x| x == id).unwrap();
        assert!(pos(c) < pos(b), "C before B: {order:?}");
        assert!(pos(b) < pos(a), "B before A: {order:?}");
    }

    #[test]
    fn instance_paint_attempt_is_a_typed_error_naming_the_source() {
        let mut stack = LayerStack::new();
        let src = stack.add_layer("Base", LayerKind::Paint);
        let inst = stack.add_instance("inst", src).expect("source exists");
        assert_eq!(stack.check_paintable(src), Ok(()));
        assert_eq!(
            stack.check_paintable(inst),
            Err(InstanceError::PaintOnInstance {
                instance: inst,
                source: src,
            })
        );
        assert_eq!(
            stack.check_paintable(999),
            Err(InstanceError::UnknownLayer(999))
        );
    }

    #[test]
    fn instance_unknown_source_and_not_an_instance_are_typed() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);
        assert!(matches!(
            stack.add_instance("ghost", 999),
            Err(InstanceError::UnknownSource { .. })
        ));
        assert_eq!(
            stack.set_instance_source(a, a),
            Err(InstanceError::NotAnInstance(a))
        );
        let inst = stack.add_instance("inst", a).expect("source exists");
        assert!(matches!(
            stack.set_instance_source(inst, 999),
            Err(InstanceError::UnknownSource { .. })
        ));
    }

    #[test]
    fn instance_retarget_round_trips_through_undo() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);
        let b = stack.add_layer("B", LayerKind::Paint);
        let inst = stack.add_instance("inst", a).expect("source exists");
        let mut undo: UndoStack<LayerCommand> = UndoStack::new(100);
        undo.push(LayerCommand::set_instance_source(inst, b), &mut stack);
        assert_eq!(stack.instance_source(inst), Some(b));
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.instance_source(inst), Some(a));
        assert!(undo.redo(&mut stack));
        assert_eq!(stack.instance_source(inst), Some(b));
    }
}
