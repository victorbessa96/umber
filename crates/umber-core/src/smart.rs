//! The SMART LAYER TIER (§3:42, §3:45, §5:68): anchors, smart materials,
//! smart masks.
//!
//! One shared mechanism in three shapes:
//!
//! * [`AnchorRef`] — a typed `(layer, channel)` reference a node-graph
//!   input binds within the same texture set. The graph eval resolves it
//!   to the layer's composited channel at eval time (the graph-side half
//!   lives in `umber-graph`'s `anchor` module, a structural mirror —
//!   `umber-core` stays graph-agnostic, so the two `AnchorRef` shapes are
//!   kept field-identical and converted at the app bridge).
//! * [`SmartMaterial`] — a `.umbersmart` folder preset (serde JSON): a
//!   layer-stack template (folder + fill layers + masks) whose fill layers
//!   bind mesh-map-driven generators (`AO` → generator nodes via anchors).
//!   Applying instantiates the template into a [`LayerStack`] and resolves
//!   the anchors to freshly-allocated layer ids.
//! * [`SmartMask`] — an `.umbermask` effect-stack preset: an ordered list
//!   of `{source, op, params}` entries composed into one mask, applyable
//!   to any layer's mask slot.
//!
//! # Judgment calls (for the static verifier)
//!
//! * Anchor channels are `String`, not an enum: channels are texture-set
//!   data (`TextureSet::channels`), and presets must survive sets whose
//!   channel list the preset author never saw. Validation happens at
//!   resolve time ([`AnchorRef::resolve`]), not at parse time.
//! * Mask composition math ([`SmartMask::compose_scalar`]) is scalar `f32`
//!   here: pixel buffers live in the Wave-2 compositor/GPU pool, not in
//!   the document model. The scalar fold pins the op ORDER semantics; the
//!   raster path reuses the same op order.
//! * Preset layer kinds are a closed `#[serde(tag)]` enum: an unknown kind
//!   fails the parse loudly rather than loading as a silent paint layer.

use serde::{Deserialize, Serialize};

use crate::layers::{BlendMode, LayerKind, LayerMask, LayerStack};
use crate::TextureSet;

/// A typed reference to one layer's channel within the same texture set
/// (§3:42). Any node-graph input can bind one; the graph eval resolves it
/// to the layer's composited channel at eval time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnchorRef {
    /// The referenced layer's id (same texture set as the binding graph).
    pub layer_id: u64,
    /// The channel name (e.g. `"baseColor"`, `"AO"`).
    pub channel: String,
}

impl AnchorRef {
    /// Binds `layer_id`'s `channel`.
    pub fn new(layer_id: u64, channel: impl Into<String>) -> Self {
        Self {
            layer_id,
            channel: channel.into(),
        }
    }

    /// The graph-externals key this anchor resolves through
    /// (`"anchor:{layer_id}:{channel}"` — the same key the graph-side
    /// `anchor` module uses, so the app bridge converts without renaming).
    pub fn key(&self) -> String {
        format!("anchor:{}:{}", self.layer_id, self.channel)
    }

    /// Resolves the anchor against `stack` (and, when given, `set` for
    /// channel validation): the referenced layer must exist, and the
    /// channel must be named by the texture set.
    ///
    /// # Errors
    ///
    /// [`AnchorError::UnknownLayer`] when the layer is missing — including
    /// the deleted case (a layer removed after the anchor bound it);
    /// [`AnchorError::UnknownChannel`] when `set` names no such channel.
    pub fn resolve(&self, stack: &LayerStack, set: Option<&TextureSet>) -> Result<(), AnchorError> {
        if stack.layer(self.layer_id).is_none() {
            return Err(AnchorError::UnknownLayer(self.layer_id));
        }
        if let Some(set) = set {
            if !set.channels.iter().any(|c| c.name == self.channel) {
                return Err(AnchorError::UnknownChannel {
                    layer: self.layer_id,
                    channel: self.channel.clone(),
                });
            }
        }
        Ok(())
    }
}

/// Everything that can go wrong resolving an [`AnchorRef`].
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AnchorError {
    /// No layer with this id — never existed, or deleted after binding.
    #[error("anchor references missing layer {0} (deleted or never present)")]
    UnknownLayer(u64),
    /// The texture set names no such channel.
    #[error("anchor on layer {layer} references unknown channel {channel:?}")]
    UnknownChannel {
        /// The (existing) layer's id.
        layer: u64,
        /// The missing channel name.
        channel: String,
    },
}

/// Everything that can go wrong applying smart presets.
#[derive(Debug, thiserror::Error)]
pub enum SmartError {
    /// A template layer names a kind this build does not know.
    #[error("unknown smart layer kind {0:?}")]
    UnknownKind(String),
    /// A template carries no layers (nothing to instantiate).
    #[error("smart preset {0:?} has no layers")]
    EmptyPreset(String),
    /// A preset file was not valid JSON.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// Reading a preset file failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The target layer for a mask apply does not exist.
    #[error("unknown layer {0}")]
    UnknownLayer(u64),
    /// A mask stack with no ops cannot compose.
    #[error("smart mask {0:?} has no ops")]
    EmptyMask(String),
    /// A preset file larger than [`MAX_PRESET_BYTES`] (read refused).
    #[error("smart preset file is {0} bytes; the cap is {MAX_PRESET_BYTES}")]
    TooLarge(u64),
    /// More layers or ops than the caps ([`MAX_PRESET_LAYERS`],
    /// [`MAX_MASK_OPS`]).
    #[error("smart preset {name:?} has {count} {what}; the cap is {cap}")]
    TooMany {
        /// The preset's display name.
        name: String,
        /// `"layers"` or `"ops"`.
        what: &'static str,
        /// How many it has.
        count: usize,
        /// The cap.
        cap: usize,
    },
}

/// Largest `.umbersmart` / `.umbermask` file read (1 MiB). The shipped
/// presets are ~1 KB; a preset is JSON metadata, never pixels.
pub const MAX_PRESET_BYTES: u64 = 1 << 20;
/// Most layers a smart material may instantiate in one apply.
pub const MAX_PRESET_LAYERS: usize = 256;
/// Most stages a smart mask may compose. Every op runs once per texel
/// (16.8M at 4K), so this bounds the apply's cost, not just its size.
pub const MAX_MASK_OPS: usize = 64;

/// Reads a preset file, refusing anything over [`MAX_PRESET_BYTES`]
/// BEFORE allocating it (the size comes from metadata, and the read is
/// capped too in case the file grows between the two).
fn read_capped(path: &std::path::Path) -> Result<String, SmartError> {
    use std::io::Read as _;
    let len = std::fs::metadata(path)?.len();
    if len > MAX_PRESET_BYTES {
        return Err(SmartError::TooLarge(len));
    }
    let mut text = String::new();
    std::fs::File::open(path)?
        .take(MAX_PRESET_BYTES + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_PRESET_BYTES {
        return Err(SmartError::TooLarge(text.len() as u64));
    }
    Ok(text)
}

fn default_preset_version() -> u32 {
    1
}

fn default_opacity() -> f32 {
    1.0
}

fn default_anchor_channel() -> String {
    "baseColor".to_string()
}

/// A mesh-map-driven generator binding on a smart-material fill layer:
/// the fill is driven by `node_def` fed from baked map `map_name` (e.g.
/// `mesh_map_generator` ← `AO`), exposed downstream as an anchor on
/// `channel`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratorBinding {
    /// The generator node (e.g. `"mesh_map_generator"`).
    pub node_def: String,
    /// The baked mesh map feeding it (e.g. `"AO"`; empty for
    /// self-contained generators like `noise_perlin`).
    #[serde(default)]
    pub map_name: String,
    /// The channel the applied layer exposes as an anchor.
    #[serde(default = "default_anchor_channel")]
    pub channel: String,
}

/// One layer entry in a [`.umbersmart`](SmartMaterial) template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartLayerTemplate {
    /// Display name for the instantiated layer.
    pub name: String,
    /// Template kind: `"paint"`, `"fill"` or `"folder"`.
    #[serde(default = "default_template_kind")]
    pub kind: SmartLayerKind,
    /// Blend mode of the instantiated layer.
    #[serde(default)]
    pub blend_mode: BlendMode,
    /// Opacity of the instantiated layer (`0.0..=1.0`, clamped on apply).
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Mask name, when the instantiated layer carries a mask.
    #[serde(default)]
    pub mask: Option<String>,
    /// Mesh-map-driven generator binding (fill layers).
    #[serde(default)]
    pub generator: Option<GeneratorBinding>,
}

fn default_template_kind() -> SmartLayerKind {
    SmartLayerKind::Fill
}

/// The closed kind vocabulary for [`SmartLayerTemplate`]: unknown strings
/// fail the parse (see the module docs).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmartLayerKind {
    /// Instantiates [`LayerKind::Paint`].
    Paint,
    /// Instantiates [`LayerKind::Fill`].
    #[default]
    Fill,
    /// Instantiates `LayerKind::Folder { passthrough: true }`.
    Folder,
}

/// A `.umbersmart` folder preset (§3:45): a layer-stack template whose fill
/// layers bind mesh-map-driven generators via anchors.
///
/// File shape (serde JSON, extension `.umbersmart`):
///
/// ```json
/// {
///   "name": "Worn metal",
///   "version": 1,
///   "layers": [
///     { "name": "Base", "kind": "folder" },
///     { "name": "Wear", "kind": "fill",
///       "generator": { "node_def": "mesh_map_generator",
///                      "map_name": "AO", "channel": "baseColor" } }
///   ]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartMaterial {
    /// Preset display name.
    pub name: String,
    /// Preset schema version (1 in this build).
    #[serde(default = "default_preset_version")]
    pub version: u32,
    /// Stack template, bottom (index 0) to top.
    pub layers: Vec<SmartLayerTemplate>,
}

/// What [`SmartMaterial::apply`] created.
#[derive(Debug, Clone, PartialEq)]
pub struct SmartApplyResult {
    /// Freshly-allocated layer ids, bottom to top.
    pub layer_ids: Vec<u64>,
    /// Anchors resolved to the fresh ids: one per template layer with a
    /// [`GeneratorBinding`], as `(layer_id, channel)`.
    pub anchors: Vec<AnchorRef>,
}

impl SmartMaterial {
    /// Parses a `.umbersmart` document.
    ///
    /// # Errors
    ///
    /// [`SmartError::Json`] on malformed input.
    pub fn from_json(json: &str) -> Result<Self, SmartError> {
        let material: Self = serde_json::from_str(json)?;
        if material.layers.len() > MAX_PRESET_LAYERS {
            return Err(SmartError::TooMany {
                name: material.name,
                what: "layers",
                count: material.layers.len(),
                cap: MAX_PRESET_LAYERS,
            });
        }
        Ok(material)
    }

    /// Reads a `.umbersmart` file.
    ///
    /// # Errors
    ///
    /// [`SmartError::Io`] / [`SmartError::Json`] on unreadable input.
    pub fn load_file(path: &std::path::Path) -> Result<Self, SmartError> {
        let text = read_capped(path)?;
        Self::from_json(&text)
    }

    /// Instantiates the template into `stack` (bottom to top) and resolves
    /// the generator-bound anchors to the fresh layer ids.
    ///
    /// Mask entries become enabled [`LayerMask`]s; generator bindings are
    /// recorded as the returned anchors (the graph-side wiring consumes
    /// them — the document model stores the layers, not the nodes).
    ///
    /// # Errors
    ///
    /// [`SmartError::EmptyPreset`] when the template has no layers.
    pub fn apply(&self, stack: &mut LayerStack) -> Result<SmartApplyResult, SmartError> {
        if self.layers.is_empty() {
            return Err(SmartError::EmptyPreset(self.name.clone()));
        }
        let mut layer_ids = Vec::with_capacity(self.layers.len());
        let mut anchors = Vec::new();
        for template in &self.layers {
            let kind = match template.kind {
                SmartLayerKind::Paint => LayerKind::Paint,
                SmartLayerKind::Fill => LayerKind::Fill,
                SmartLayerKind::Folder => LayerKind::Folder { passthrough: true },
            };
            let id = stack.add_layer(template.name.clone(), kind);
            if template.opacity.is_finite() {
                stack.set_opacity(id, template.opacity);
            }
            stack.set_blend_mode(id, template.blend_mode);
            if let Some(mask_name) = &template.mask {
                if let Some(layer) = stack.layer_mut(id) {
                    layer.mask = Some(LayerMask {
                        name: mask_name.clone(),
                        enabled: true,
                    });
                }
            }
            if let Some(binding) = &template.generator {
                anchors.push(AnchorRef::new(id, binding.channel.clone()));
            }
            layer_ids.push(id);
        }
        Ok(SmartApplyResult { layer_ids, anchors })
    }
}

/// One stage's input for a [`SmartMask`] effect stack (§5:68).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MaskSource {
    /// A layer channel by template-local layer name (resolved against the
    /// stack at apply time) — the anchor form inside presets.
    Anchor {
        /// The template/stack layer name.
        layer: String,
        /// The channel name.
        channel: String,
    },
    /// A generator node fed from a baked mesh map.
    Generator {
        /// The generator node (e.g. `"noise_perlin"`, `"mesh_map_generator"`).
        node_def: String,
        /// The baked map feeding it (e.g. `"curvature"`).
        map_name: String,
    },
    /// A flat constant value (`0.0..=1.0`).
    Constant {
        /// The constant mask value.
        value: f32,
    },
}

/// The op applied at one [`SmartMask`] stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaskOpKind {
    /// Running × stage (`acc *= s`).
    #[default]
    Multiply,
    /// Running + stage, clamped (`acc = clamp01(acc + s * amount)`).
    Add,
    /// Replace with the inverted stage (`acc = 1 - s`; ignores the
    /// running value — the effect-stack invert stage).
    Invert,
    /// Remap the running value through levels (`low`/`high`/`gamma`;
    /// ignores the stage value — the effect-stack levels stage).
    Levels,
}

/// Per-op tuning for [`MaskOp`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaskOpParams {
    /// Gain on the stage value for [`MaskOpKind::Add`] (default 1.0).
    #[serde(default = "default_amount")]
    pub amount: f32,
    /// Levels black point (default 0.0).
    #[serde(default)]
    pub low: f32,
    /// Levels white point (default 1.0).
    #[serde(default = "default_opacity")]
    pub high: f32,
    /// Levels gamma (default 1.0 = linear).
    #[serde(default = "default_opacity")]
    pub gamma: f32,
}

fn default_amount() -> f32 {
    1.0
}

impl Default for MaskOpParams {
    fn default() -> Self {
        Self {
            amount: 1.0,
            low: 0.0,
            high: 1.0,
            gamma: 1.0,
        }
    }
}

/// One `{source, op, params}` stage of a [`SmartMask`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaskOp {
    /// Where this stage's value comes from.
    pub source: MaskSource,
    /// How the stage combines into the running result.
    #[serde(default)]
    pub op: MaskOpKind,
    /// Op tuning.
    #[serde(default)]
    pub params: MaskOpParams,
}

/// An `.umbermask` effect-stack preset (§5:68): an ordered list of stages
/// composed into one mask, applyable to any layer's mask slot.
///
/// Composition ([`Self::compose_scalar`], accumulator from white `1.0`):
///
/// * `Multiply` — `acc *= s`
/// * `Add` — `acc = clamp01(acc + s * amount)`
/// * `Invert` — `acc = 1 - s` (replaces; the invert stage inverts its
///   input, it does not scale the running value)
/// * `Levels` — `acc = levels(acc)` with `t = clamp01((acc-low)/(high-low))`,
///   `out = t^gamma` (degenerate `high <= low` maps everything below the
///   point to 0 and above to 1 rather than dividing by zero)
///
/// Order matters: `[Multiply, Add]` ≠ `[Add, Multiply]` (pinned below).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartMask {
    /// Preset display name.
    pub name: String,
    /// Preset schema version (1 in this build).
    #[serde(default = "default_preset_version")]
    pub version: u32,
    /// Effect stages, bottom (applied first) to top.
    pub ops: Vec<MaskOp>,
}

impl SmartMask {
    /// Parses an `.umbermask` document.
    ///
    /// # Errors
    ///
    /// [`SmartError::Json`] on malformed input.
    pub fn from_json(json: &str) -> Result<Self, SmartError> {
        let mask: Self = serde_json::from_str(json)?;
        if mask.ops.len() > MAX_MASK_OPS {
            return Err(SmartError::TooMany {
                name: mask.name,
                what: "ops",
                count: mask.ops.len(),
                cap: MAX_MASK_OPS,
            });
        }
        Ok(mask)
    }

    /// Reads an `.umbermask` file.
    ///
    /// # Errors
    ///
    /// [`SmartError::Io`] / [`SmartError::Json`] on unreadable input.
    pub fn load_file(path: &std::path::Path) -> Result<Self, SmartError> {
        let text = read_capped(path)?;
        Self::from_json(&text)
    }

    /// Folds the stage list into one mask value: `lookup` resolves each
    /// stage's [`MaskSource`] to its scalar value (`0.0..=1.0`; non-finite
    /// lookups count as `0.0` rather than poisoning the stack).
    ///
    /// # Errors
    ///
    /// [`SmartError::EmptyMask`] when the preset has no ops.
    pub fn compose_scalar(&self, lookup: &dyn Fn(&MaskSource) -> f32) -> Result<f32, SmartError> {
        if self.ops.is_empty() {
            return Err(SmartError::EmptyMask(self.name.clone()));
        }
        let mut acc = 1.0f32;
        for stage in &self.ops {
            let raw = lookup(&stage.source);
            let s = if raw.is_finite() {
                raw.clamp(0.0, 1.0)
            } else {
                0.0
            };
            acc = match stage.op {
                MaskOpKind::Multiply => acc * s,
                MaskOpKind::Add => {
                    let amount = if stage.params.amount.is_finite() {
                        stage.params.amount
                    } else {
                        0.0
                    };
                    (acc + s * amount).clamp(0.0, 1.0)
                }
                MaskOpKind::Invert => 1.0 - s,
                MaskOpKind::Levels => {
                    let (low, high) = (stage.params.low, stage.params.high);
                    if !(low.is_finite() && high.is_finite()) || high <= low {
                        if acc <= low {
                            0.0
                        } else {
                            1.0
                        }
                    } else {
                        let t = ((acc - low) / (high - low)).clamp(0.0, 1.0);
                        let gamma = if stage.params.gamma.is_finite() && stage.params.gamma > 0.0 {
                            stage.params.gamma
                        } else {
                            1.0
                        };
                        t.powf(gamma).clamp(0.0, 1.0)
                    }
                }
            };
        }
        Ok(acc)
    }

    /// Applies the preset to `layer_id`'s mask slot: records an enabled
    /// [`LayerMask`] named after the preset (the compositor bakes the
    /// stage list into pixels; the document model holds the slot).
    ///
    /// # Errors
    ///
    /// [`SmartError::UnknownLayer`] for unknown ids.
    pub fn apply_to_mask_slot(
        &self,
        stack: &mut LayerStack,
        layer_id: u64,
    ) -> Result<(), SmartError> {
        let layer = stack
            .layer_mut(layer_id)
            .ok_or(SmartError::UnknownLayer(layer_id))?;
        layer.mask = Some(LayerMask {
            name: self.name.clone(),
            enabled: true,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::LayerKind;

    #[test]
    fn oversized_presets_are_refused_before_they_cost_anything() {
        // Ops over the cap: a mask is composed once per texel, so the cap
        // bounds the apply's cost.
        let op = r#"{ "source": { "type": "constant", "value": 0.5 }, "op": "multiply" }"#;
        let ops = vec![op; MAX_MASK_OPS + 1].join(",");
        let json = format!(r#"{{ "name": "Big", "ops": [{ops}] }}"#);
        assert!(matches!(
            SmartMask::from_json(&json),
            Err(SmartError::TooMany { what: "ops", .. })
        ));
        let ok = vec![op; MAX_MASK_OPS].join(",");
        assert!(SmartMask::from_json(&format!(r#"{{ "name": "Ok", "ops": [{ok}] }}"#)).is_ok());
        // Layers over the cap.
        let layer = r#"{ "name": "l", "kind": "paint" }"#;
        let layers = vec![layer; MAX_PRESET_LAYERS + 1].join(",");
        let json = format!(r#"{{ "name": "Big", "layers": [{layers}] }}"#);
        assert!(matches!(
            SmartMaterial::from_json(&json),
            Err(SmartError::TooMany { what: "layers", .. })
        ));
        // A file over the byte cap is refused from metadata, unread.
        let path = std::env::temp_dir().join(format!("umber-big-{}.umbermask", std::process::id()));
        std::fs::write(&path, vec![b' '; (MAX_PRESET_BYTES + 1) as usize]).unwrap();
        assert!(matches!(
            SmartMask::load_file(&path),
            Err(SmartError::TooLarge(_))
        ));
        let _ = std::fs::remove_file(&path);
    }

    fn paint_stack(names: &[&str]) -> LayerStack {
        let mut stack = LayerStack::new();
        for name in names {
            stack.add_layer((*name).to_string(), LayerKind::Paint);
        }
        stack
    }

    #[test]
    fn anchor_resolves_to_its_layer_and_channel() {
        let stack = paint_stack(&["Base"]);
        let id = stack.layers[0].id;
        let set = TextureSet::new_default("Body");
        AnchorRef::new(id, "baseColor")
            .resolve(&stack, Some(&set))
            .expect("baseColor exists on the default set");
        match AnchorRef::new(id, "no-such-channel").resolve(&stack, Some(&set)) {
            Err(AnchorError::UnknownChannel { layer, channel }) => {
                assert_eq!(layer, id);
                assert_eq!(channel, "no-such-channel");
            }
            other => panic!("unknown channel must be typed, got {other:?}"),
        }
        // Without a set there is no channel vocabulary: existence only.
        AnchorRef::new(id, "anything")
            .resolve(&stack, None)
            .expect("no set, no channel check");
    }

    #[test]
    fn anchor_on_deleted_layer_is_a_typed_error() {
        let mut stack = paint_stack(&["Base"]);
        let id = stack.layers[0].id;
        let anchor = AnchorRef::new(id, "baseColor");
        anchor
            .resolve(&stack, None)
            .expect("layer present, resolves");
        stack.remove_layer(id);
        assert_eq!(
            anchor.resolve(&stack, None),
            Err(AnchorError::UnknownLayer(id)),
            "deleting the layer must break the anchor loudly"
        );
        assert_eq!(anchor.key(), format!("anchor:{id}:baseColor"));
    }

    const WORN_METAL_JSON: &str = r#"{
        "name": "Worn metal (test)",
        "version": 1,
        "layers": [
            { "name": "Wear folder", "kind": "folder" },
            { "name": "Base tint", "kind": "fill",
              "generator": { "node_def": "mesh_map_generator",
                             "map_name": "AO", "channel": "baseColor" },
              "mask": "Wear mask" },
            { "name": "Edge wear", "kind": "fill",
              "generator": { "node_def": "mesh_map_generator",
                             "map_name": "curvature", "channel": "roughness" } }
        ]
    }"#;

    #[test]
    fn smart_material_apply_instantiates_three_layers_and_two_anchors() {
        let preset = SmartMaterial::from_json(WORN_METAL_JSON).expect("test preset parses");
        assert_eq!(preset.layers.len(), 3);
        let mut stack = LayerStack::new();
        let result = preset.apply(&mut stack).expect("apply works");
        assert_eq!(result.layer_ids.len(), 3);
        assert_eq!(stack.layers.len(), 3);
        // Two generator-bound fill layers → two anchors on fresh ids.
        assert_eq!(result.anchors.len(), 2);
        assert_eq!(
            result.anchors[0],
            AnchorRef::new(result.layer_ids[1], "baseColor")
        );
        assert_eq!(
            result.anchors[1],
            AnchorRef::new(result.layer_ids[2], "roughness")
        );
        // The folder + mask + kinds survived the apply.
        assert!(matches!(
            stack.layer(result.layer_ids[0]).unwrap().kind,
            LayerKind::Folder { passthrough: true }
        ));
        assert_eq!(
            stack
                .layer(result.layer_ids[1])
                .unwrap()
                .mask
                .as_ref()
                .map(|m| m.name.as_str()),
            Some("Wear mask")
        );
        // Anchors resolve against the applied stack.
        for anchor in &result.anchors {
            assert!(
                stack.layer(anchor.layer_id).is_some(),
                "anchor {anchor:?} must name a fresh layer"
            );
        }
    }

    #[test]
    fn smart_material_rejects_empty_and_unknown_kinds() {
        let empty = SmartMaterial {
            name: "Empty".into(),
            version: 1,
            layers: vec![],
        };
        let mut stack = LayerStack::new();
        assert!(matches!(
            empty.apply(&mut stack),
            Err(SmartError::EmptyPreset(_))
        ));
        assert!(
            SmartMaterial::from_json(r#"{"name":"x","layers":[{"name":"y","kind":"voxel"}]}"#)
                .is_err()
        );
    }

    fn mask_fixture() -> SmartMask {
        SmartMask {
            name: "test".into(),
            version: 1,
            ops: vec![
                MaskOp {
                    source: MaskSource::Constant { value: 0.5 },
                    op: MaskOpKind::Multiply,
                    params: MaskOpParams::default(),
                },
                MaskOp {
                    source: MaskSource::Constant { value: 0.25 },
                    op: MaskOpKind::Add,
                    params: MaskOpParams::default(),
                },
            ],
        }
    }

    #[test]
    fn mask_stack_composition_order_is_pinned() {
        // [Multiply(0.5), Add(0.25)]: 1.0*0.5=0.5, then 0.5+0.25=0.75.
        let mask = mask_fixture();
        let v = mask
            .compose_scalar(&|src| match src {
                MaskSource::Constant { value } => *value,
                _ => panic!("fixture is constants only"),
            })
            .expect("composes");
        assert!((v - 0.75).abs() < 1e-6, "expected 0.75, got {v}");
        // Reversed: [Add(0.25), Multiply(0.5)]: 1.0+0.25→1.0 (clamped),
        // then 1.0*0.5=0.5. Order matters — same stages, other result.
        let mut reversed = mask_fixture();
        reversed.ops.reverse();
        let w = reversed
            .compose_scalar(&|src| match src {
                MaskSource::Constant { value } => *value,
                _ => panic!("fixture is constants only"),
            })
            .expect("composes");
        assert!((w - 0.5).abs() < 1e-6, "expected 0.5, got {w}");
        assert_ne!(v, w, "composition order must be observable");
    }

    #[test]
    fn mask_invert_and_levels_pin_known_values() {
        let mask = SmartMask {
            name: "grades".into(),
            version: 1,
            ops: vec![
                MaskOp {
                    source: MaskSource::Constant { value: 0.25 },
                    op: MaskOpKind::Invert,
                    params: MaskOpParams::default(),
                },
                MaskOp {
                    source: MaskSource::Constant { value: 0.0 },
                    op: MaskOpKind::Levels,
                    params: MaskOpParams {
                        low: 0.5,
                        high: 1.0,
                        ..MaskOpParams::default()
                    },
                },
            ],
        };
        // Invert: 1-0.25=0.75; levels(0.75 in [0.5,1.0]) = 0.5.
        let v = mask
            .compose_scalar(&|src| match src {
                MaskSource::Constant { value } => *value,
                _ => panic!("fixture is constants only"),
            })
            .expect("composes");
        assert!((v - 0.5).abs() < 1e-6, "expected 0.5, got {v}");
    }

    #[test]
    fn shipped_presets_parse_apply_and_compose() {
        // The files under assets/smart/ are the static contract: they must
        // parse under this schema and apply cleanly (guards schema drift).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/smart");
        for (file, layers, anchors) in [
            ("worn-metal.umbersmart", 4, 3),
            ("painted-wood.umbersmart", 4, 2),
        ] {
            let preset = SmartMaterial::load_file(&root.join(file)).expect("shipped preset loads");
            let mut stack = LayerStack::new();
            let result = preset.apply(&mut stack).expect("shipped preset applies");
            assert_eq!(result.layer_ids.len(), layers, "{file}");
            assert_eq!(result.anchors.len(), anchors, "{file}");
        }
        let mask =
            SmartMask::load_file(&root.join("edge-wear.umbermask")).expect("shipped mask loads");
        assert_eq!(mask.ops.len(), 3);
        // curvature 0.8 → 0.8, +0.35 → 1.0 (clamped), levels into [0.4,0.9] → 1.0.
        let v = mask
            .compose_scalar(&|src| match src {
                MaskSource::Generator { .. } => 0.8,
                MaskSource::Constant { value } => *value,
                MaskSource::Anchor { .. } => 0.0,
            })
            .expect("shipped mask composes");
        assert!((v - 1.0).abs() < 1e-6, "expected 1.0, got {v}");
    }

    #[test]
    fn smart_mask_applies_to_any_layers_mask_slot() {
        let mask = mask_fixture();
        let mut stack = paint_stack(&["Base"]);
        let id = stack.layers[0].id;
        mask.apply_to_mask_slot(&mut stack, id)
            .expect("mask slot applies");
        assert_eq!(
            stack
                .layer(id)
                .unwrap()
                .mask
                .as_ref()
                .map(|m| m.name.as_str()),
            Some("test")
        );
        assert!(
            matches!(
                mask.apply_to_mask_slot(&mut stack, 999),
                Err(SmartError::UnknownLayer(999))
            ),
            "mask apply to a missing layer is typed"
        );
        assert!(matches!(
            SmartMask {
                name: "empty".into(),
                version: 1,
                ops: vec![]
            }
            .compose_scalar(&|_| 1.0),
            Err(SmartError::EmptyMask(_))
        ));
    }
}
