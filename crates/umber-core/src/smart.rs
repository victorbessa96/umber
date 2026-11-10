//! The SMART LAYER TIER (§3:42, §3:45, §5:68): anchors, smart materials,
//! smart masks, smart filters.
//!
//! One shared mechanism in four shapes:
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
//!   the anchors to freshly-allocated layer ids. Its optional `parameters`
//!   block declares tweakable [`SmartParam`]s, and a generator binding's
//!   `param` names one — that is how a node input (wear amount, rust
//!   coverage, an oxide mode) is driven by a declared control instead of a
//!   hard-coded value.
//! * [`SmartMask`] — an `.umbermask` effect-stack preset: an ordered list
//!   of `{source, op, params}` entries composed into one mask, applyable
//!   to any layer's mask slot.
//! * [`SmartFilter`] — an `.umberfilter` preset: a named, ordered list of
//!   parameterized image ops ([`FilterOp`]) with a pure-CPU, deterministic
//!   [`SmartFilter::apply`] over an RGBA8 buffer. Core-only for now: wiring
//!   it into the assets panel / drag payloads / CLI is the follow-up kill
//!   (assets-panel gap #2 in `docs/loop/briefs/2026-11-05-assets-panel-
//!   starter-content.md`); [`smart_kind_for_path`] already recognizes the
//!   extension so that gating has a hook.
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
//!   The same discipline holds for [`FilterOp`]: an unknown op tag is a
//!   typed parse error, never a silent no-op. Filter params are validated
//!   finite at parse and clamped into their ranges there, and `apply`
//!   re-clamps defensively — a hostile `.umberfilter` can produce a typed
//!   `Err` or a clamped no-op, never a panic or a non-deterministic pixel.
//! * Filter ops act on RGB only and preserve alpha; the filter is a color
//!   operation on a layer, not an alpha operation.
//! * Parameters are declared on the MATERIAL, not per layer (4.1a,
//!   2026-11-10). One flat id namespace lets several generator bindings
//!   share one control (one "wear" driving two layers) without duplicate
//!   declarations, and the inspector half (4.1b) presents one control set
//!   per applied preset. A per-layer block would fork the namespace and
//!   force the UI to merge duplicates; the brief names this fork and asks
//!   for one arm only.
//! * `SmartMaterial::version` stays 1 with the parameters block. The block
//!   is additive and optional (`#[serde(default, skip_serializing_if)]`),
//!   and no struct on this path sets `deny_unknown_fields` (checked in the
//!   4.1a kill), so an old reader ignores `parameters`/`param` and still
//!   instantiates the layer stack — it only loses the controls. Old files
//!   parse with `parameters: []`. A version bump would strand every shipped
//!   preset for a purely additive field, so the compat note lives here.
//!   The `param` key is skipped when absent so specs without it (all other
//!   starter materials) re-serialize byte-identically.
//! * Parameter declarations are validated loudly at parse AND apply
//!   (duplicate ids, empty labels, a default outside its range, a choice
//!   default not among its options, a generator `param` naming an unknown
//!   id) — these are authored metadata, so a clamp would hide the author's
//!   mistake. Filter runtime params clamp instead because there the value
//!   is per-pixel input, not a declaration.

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
    /// A filter op with no ops cannot apply.
    #[error("smart filter {0:?} has no ops")]
    EmptyFilter(String),
    /// A filter op parameter parsed as a non-finite float (a JSON number
    /// too large for `f32` saturates to ±inf; `"NaN"` strings fail the
    /// parse itself). Refused at parse, never stored.
    #[error("smart filter {name:?} op {op} has non-finite {param}")]
    NonFiniteParam {
        /// The filter's display name.
        name: String,
        /// The op's index in the op list.
        op: usize,
        /// The offending parameter (`"low"`, `"gamma"`, `"point"`, …).
        param: &'static str,
    },
    /// The RGBA buffer passed to [`SmartFilter::apply`] does not match
    /// `width * height * 4`.
    #[error("RGBA buffer is {actual} bytes; {expected} expected for the given dimensions")]
    BadBuffer {
        /// `width * height * 4`.
        expected: usize,
        /// The buffer's actual length.
        actual: usize,
    },
    /// A generator binding's `param` names an id the preset never declares.
    /// Refused at parse and at apply — never a silent ignore.
    #[error("smart material {material:?} layer {layer:?} binds unknown parameter {param:?}")]
    UnknownParam {
        /// The preset's display name.
        material: String,
        /// The template layer whose generator carries the dangling ref.
        layer: String,
        /// The undeclared parameter id.
        param: String,
    },
    /// A declared parameter id lookup found nothing ([`SmartMaterial::param`]).
    #[error("smart material {material:?} declares no parameter {param:?}")]
    UnknownParamId {
        /// The preset's display name.
        material: String,
        /// The undeclared parameter id.
        param: String,
    },
    /// A parameter declaration is malformed. `why` is a fixed phrase so the
    /// caller can pin it in a test.
    #[error("smart material {material:?} parameter {param:?} is invalid: {why}")]
    BadParam {
        /// The preset's display name.
        material: String,
        /// The offending parameter id (possibly empty).
        param: String,
        /// What is wrong, in a fixed phrase.
        why: &'static str,
    },
}

/// Largest `.umbersmart` / `.umbermask` / `.umberfilter` file read
/// (1 MiB). The shipped presets are ~1 KB; a preset is JSON metadata,
/// never pixels.
pub const MAX_PRESET_BYTES: u64 = 1 << 20;
/// Most layers a smart material may instantiate in one apply.
pub const MAX_PRESET_LAYERS: usize = 256;
/// Most parameters a smart material may declare. A parameter block is UI
/// metadata, one control per entry; 64 is far past any shipped preset and
/// keeps a hostile file from making the inspector build thousands of rows.
pub const MAX_PRESET_PARAMS: usize = 64;
/// Most options a [`SmartParamKind::Choice`] may offer (same reasoning as
/// [`MAX_PRESET_PARAMS`]: a combo box, not a database).
pub const MAX_PARAM_CHOICES: usize = 64;
/// Most stages a smart mask may compose. Every op runs once per texel
/// (16.8M at 4K), so this bounds the apply's cost, not just its size.
pub const MAX_MASK_OPS: usize = 64;
/// Most ops a smart filter may chain. Each op is a full-buffer pass
/// (16.8M texels at 4K), so this bounds the apply's cost.
pub const MAX_FILTER_OPS: usize = 64;
/// Largest box-blur radius a [`FilterOp::Blur`] accepts (clamped down to
/// this at parse; the apply is O(pixels), the radius only widens the
/// window).
pub const MAX_BLUR_RADIUS: u32 = 64;
/// Most control points a [`FilterOp::Curves`] accepts.
pub const MAX_CURVE_POINTS: usize = 32;
/// Posterize band count is clamped into `2..=256` (256 = identity).
pub const MAX_POSTERIZE_LEVELS: u32 = 256;
/// Levels gamma is clamped into `0.1..=10.0` (0 would be a div-by-zero
/// in the exponent's inverse; the mask path's degenerate guard does not
/// fit a stored param).
pub const MIN_GAMMA: f32 = 0.1;
/// See [`MIN_GAMMA`].
pub const MAX_GAMMA: f32 = 10.0;

/// The `.umbersmart` file extension (without the dot).
pub const SMART_MATERIAL_EXT: &str = "umbersmart";
/// The `.umbermask` file extension (without the dot).
pub const SMART_MASK_EXT: &str = "umbermask";
/// The `.umberfilter` file extension (without the dot).
pub const SMART_FILTER_EXT: &str = "umberfilter";

/// Which smart-asset kind a path is, by extension (case-insensitive) —
/// the `env::format_for_path`-style gate for the smart tier, so the app's
/// library scan / import gating has one core-side hook. Returns `None`
/// for anything that isn't a smart preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SmartAssetKind {
    /// A `.umbersmart` layer-stack template ([`SmartMaterial`]).
    Material,
    /// An `.umbermask` effect-stack preset ([`SmartMask`]).
    Mask,
    /// A `.umberfilter` image-op preset ([`SmartFilter`]).
    Filter,
}

/// Picks the smart-asset kind from the file extension
/// (case-insensitive), mirroring `env::format_for_path` in umber-app:
/// the extension is the gate, content is never sniffed here.
pub fn smart_kind_for_path(path: &std::path::Path) -> Option<SmartAssetKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        SMART_MATERIAL_EXT => Some(SmartAssetKind::Material),
        SMART_MASK_EXT => Some(SmartAssetKind::Mask),
        SMART_FILTER_EXT => Some(SmartAssetKind::Filter),
        _ => None,
    }
}

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
    /// The declared parameter id driving this generator's input (a wear
    /// amount, a coverage float, an oxide mode — whatever the node calls
    /// its primary input). `None` = the input is fixed by the template.
    ///
    /// Resolved against [`SmartMaterial::parameters`] at parse and apply
    /// ([`SmartMaterial::resolve_binding`]); an unknown id is a typed
    /// [`SmartError::UnknownParam`], never a silent ignore. Skipped when
    /// absent so presets without parameters re-serialize byte-identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
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

/// The value vocabulary of a [`SmartParam`].
///
/// Closed (`#[serde(tag = "type")]`): an unknown `type` fails the parse
/// loudly, never loads as a silent float. `Bool` is deliberately absent —
/// none of the shipped materials needs a two-state control yet, and the
/// enum is cheap to extend when one does. Adding a variant later is
/// additive for readers that know it and a loud typed
/// [`SmartError::Json`] for readers that do not — never a misread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SmartParamKind {
    /// A bounded float control.
    Float {
        /// Lower bound (inclusive), finite.
        min: f32,
        /// Upper bound (inclusive), finite, `>= min`.
        max: f32,
        /// Slider step, finite and `> 0`.
        step: f32,
        /// Starting value, within `min..=max`.
        default: f32,
    },
    /// A one-of-N string control.
    Choice {
        /// Option names: non-empty, unique, at most [`MAX_PARAM_CHOICES`].
        options: Vec<String>,
        /// Starting option; must be one of `options`.
        default: String,
    },
}

/// A parameter's current value, in the shape a generator input consumes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SmartParamValue<'a> {
    /// A [`SmartParamKind::Float`] value.
    Float(f32),
    /// The selected [`SmartParamKind::Choice`] option name.
    Choice(&'a str),
}

/// One declared, tweakable smart-material parameter (4.1a, 2026-11-10).
///
/// File shape (nested under [`SmartMaterial::parameters`]):
///
/// ```json
/// { "id": "wear", "label": "Wear amount",
///   "kind": { "type": "float", "min": 0.0, "max": 1.0,
///             "step": 0.01, "default": 0.35 } }
/// ```
///
/// `kind` is a nested object rather than a flattened one: the file already
/// nests `generator`, and a nested internally-tagged enum avoids serde's
/// flatten/tag interaction. Ids form one flat namespace per material,
/// shared by every [`GeneratorBinding::param`] reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartParam {
    /// Stable machine id, referenced by [`GeneratorBinding::param`].
    pub id: String,
    /// Human-readable control label (the 4.1b inspector renders it).
    pub label: String,
    /// The control's shape and starting value.
    pub kind: SmartParamKind,
}

impl SmartParam {
    /// The value this parameter starts at (its declared default). Core
    /// stores the declaration only; a per-instance override lives with the
    /// applied material (4.1b).
    pub fn value(&self) -> SmartParamValue<'_> {
        match &self.kind {
            SmartParamKind::Float { default, .. } => SmartParamValue::Float(*default),
            SmartParamKind::Choice { default, .. } => SmartParamValue::Choice(default),
        }
    }

    /// Structural validation, shared by the parse and apply paths. The
    /// `why` phrases are fixed so tests can pin them (module docs: this is
    /// authored metadata, so errors are loud rather than clamped).
    fn validate(&self, material: &str) -> Result<(), SmartError> {
        let bad = |why: &'static str| SmartError::BadParam {
            material: material.to_string(),
            param: self.id.clone(),
            why,
        };
        if self.id.trim().is_empty() {
            return Err(bad("empty parameter id"));
        }
        if self.label.trim().is_empty() {
            return Err(bad("empty parameter label"));
        }
        match &self.kind {
            SmartParamKind::Float {
                min,
                max,
                step,
                default,
            } => {
                for v in [*min, *max, *step, *default] {
                    if !v.is_finite() {
                        return Err(bad("non-finite float bound"));
                    }
                }
                if min > max {
                    return Err(bad("min is greater than max"));
                }
                if *step <= 0.0 {
                    return Err(bad("step must be positive"));
                }
                if *default < *min || *default > *max {
                    return Err(bad("default is outside min..=max"));
                }
            }
            SmartParamKind::Choice { options, default } => {
                if options.is_empty() {
                    return Err(bad("choice has no options"));
                }
                if options.len() > MAX_PARAM_CHOICES {
                    return Err(SmartError::TooMany {
                        name: material.to_string(),
                        what: "choices",
                        count: options.len(),
                        cap: MAX_PARAM_CHOICES,
                    });
                }
                if !options.iter().any(|option| option == default) {
                    return Err(bad("default is not one of the options"));
                }
                for (i, option) in options.iter().enumerate() {
                    if options[..i].contains(option) {
                        return Err(bad("duplicate choice option"));
                    }
                }
            }
        }
        Ok(())
    }
}

/// A `.umbersmart` folder preset (§3:45): a layer-stack template whose fill
/// layers bind mesh-map-driven generators via anchors, plus an optional
/// block of declared [`SmartParam`]s that generator bindings reference.
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
///                      "map_name": "AO", "channel": "baseColor",
///                      "param": "wear" } }
///   ],
///   "parameters": [
///     { "id": "wear", "label": "Wear amount",
///       "kind": { "type": "float", "min": 0.0, "max": 1.0,
///                 "step": 0.01, "default": 0.35 } }
///   ]
/// }
/// ```
///
/// `parameters` is optional and is emitted after `layers` (an appended
/// block, so adding controls to a preset is an additive diff); a preset
/// without it parses with an empty vec and re-serializes byte-identically.
/// `version` stays 1 — see the module docs' compat note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartMaterial {
    /// Preset display name.
    pub name: String,
    /// Preset schema version (1 in this build).
    #[serde(default = "default_preset_version")]
    pub version: u32,
    /// Stack template, bottom (index 0) to top.
    pub layers: Vec<SmartLayerTemplate>,
    /// Declared tweakable parameters, in presentation order. Empty for
    /// presets authored before 4.1a (2026-11-10) and for presets with no
    /// knobs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<SmartParam>,
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
    /// Parses a `.umbersmart` document, validating the layer cap and the
    /// parameter block (declarations and every generator `param` ref).
    ///
    /// # Errors
    ///
    /// [`SmartError::Json`] on malformed input (including unknown kinds and
    /// unknown parameter `type` tags); [`SmartError::TooMany`] past the
    /// layer / parameter / choice caps; [`SmartError::BadParam`] for a
    /// malformed declaration; [`SmartError::UnknownParam`] when a generator
    /// binding names an undeclared parameter id.
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
        material.validate_params()?;
        Ok(material)
    }

    /// Reads a `.umbersmart` file.
    ///
    /// # Errors
    ///
    /// [`SmartError::Io`] / [`SmartError::Json`] on unreadable input, plus
    /// [`Self::from_json`]'s validation errors.
    pub fn load_file(path: &std::path::Path) -> Result<Self, SmartError> {
        let text = read_capped(path)?;
        Self::from_json(&text)
    }

    /// Looks up a declared parameter by id (the inspector's binding path;
    /// generator references resolve through [`Self::resolve_binding`]).
    ///
    /// # Errors
    ///
    /// [`SmartError::UnknownParamId`] when the preset declares no such id.
    pub fn param(&self, id: &str) -> Result<&SmartParam, SmartError> {
        self.parameters
            .iter()
            .find(|param| param.id == id)
            .ok_or_else(|| SmartError::UnknownParamId {
                material: self.name.clone(),
                param: id.to_string(),
            })
    }

    /// Resolves a template layer's generator `param` reference to the
    /// declaration it names. `Ok(None)` for a layer with no generator or a
    /// fixed generator binding (`param: None`).
    ///
    /// # Errors
    ///
    /// [`SmartError::UnknownParam`] when the binding names an id the
    /// preset does not declare — the same loud path parse and apply take.
    pub fn resolve_binding(
        &self,
        template: &SmartLayerTemplate,
    ) -> Result<Option<&SmartParam>, SmartError> {
        let Some(generator) = &template.generator else {
            return Ok(None);
        };
        let Some(id) = &generator.param else {
            return Ok(None);
        };
        self.parameters
            .iter()
            .find(|param| &param.id == id)
            .map(Some)
            .ok_or_else(|| SmartError::UnknownParam {
                material: self.name.clone(),
                layer: template.name.clone(),
                param: id.clone(),
            })
    }

    /// Validates the declaration block and every generator reference —
    /// caps, duplicate ids, malformed kinds, and unknown `param` ids.
    /// Called from [`Self::from_json`] and [`Self::apply`], so a
    /// hand-built preset cannot skip it.
    fn validate_params(&self) -> Result<(), SmartError> {
        if self.parameters.len() > MAX_PRESET_PARAMS {
            return Err(SmartError::TooMany {
                name: self.name.clone(),
                what: "parameters",
                count: self.parameters.len(),
                cap: MAX_PRESET_PARAMS,
            });
        }
        for (i, param) in self.parameters.iter().enumerate() {
            param.validate(&self.name)?;
            if self.parameters[..i]
                .iter()
                .any(|other| other.id == param.id)
            {
                return Err(SmartError::BadParam {
                    material: self.name.clone(),
                    param: param.id.clone(),
                    why: "duplicate parameter id",
                });
            }
        }
        for template in &self.layers {
            self.resolve_binding(template)?;
        }
        Ok(())
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
    /// [`SmartError::EmptyPreset`] when the template has no layers;
    /// [`SmartError::BadParam`] / [`SmartError::UnknownParam`] /
    /// [`SmartError::TooMany`] when the parameter block or a generator
    /// reference is invalid — checked BEFORE any layer is added, so a
    /// refused apply leaves `stack` untouched.
    pub fn apply(&self, stack: &mut LayerStack) -> Result<SmartApplyResult, SmartError> {
        if self.layers.is_empty() {
            return Err(SmartError::EmptyPreset(self.name.clone()));
        }
        self.validate_params()?;
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

/// One parameterized image operation in a [`SmartFilter`] op list.
///
/// The vocabulary is closed (`#[serde(tag = "type")]`): an unknown `type`
/// fails the parse with a typed [`SmartError::Json`], never a silent
/// no-op. Numeric params are validated finite and clamped into range at
/// parse ([`SmartFilter::from_json`]) and re-clamped defensively in
/// [`SmartFilter::apply`] — both paths are total, neither panics.
///
/// Every op acts on the RGB channels only; alpha is preserved (a filter
/// is a color operation on a layer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FilterOp {
    /// Separable box blur over RGB, border-replicated, integer
    /// accumulation (byte-identical for same input + radius). `radius`
    /// is clamped to `0..=[MAX_BLUR_RADIUS]`; 0 is a no-op.
    Blur {
        /// Blur radius in pixels (default 1).
        #[serde(default = "default_blur_radius")]
        radius: u32,
    },
    /// Per-channel levels remap on RGB:
    /// `t = clamp01((v - low) / (high - low))`, `out = t^gamma` through a
    /// 256-entry LUT. `low`/`high` clamp to `0.0..=1.0`, `gamma` to
    /// [`MIN_GAMMA`]..=[`MAX_GAMMA`]. Degenerate `high <= low` is a step:
    /// `v <= low` → 0, else 1 (no division by zero).
    Levels {
        /// Black point (default 0.0).
        #[serde(default)]
        low: f32,
        /// White point (default 1.0).
        #[serde(default = "default_opacity")]
        high: f32,
        /// Gamma (default 1.0 = linear).
        #[serde(default = "default_opacity")]
        gamma: f32,
    },
    /// Piecewise-linear tone curve through `(x, y)` control points.
    /// Points are clamped to `0.0..=1.0` each and sorted by `x` at parse;
    /// endpoints extend flat. Empty points = identity. At most
    /// [`MAX_CURVE_POINTS`] points.
    Curves {
        /// `(x, y)` control points as `[x, y]` pairs.
        #[serde(default)]
        points: Vec<(f32, f32)>,
    },
    /// Posterize into `levels` bands, pure integer math:
    /// `q = min(levels - 1, v * levels / 256)`,
    /// `out = round(q * 255 / (levels - 1))`.
    /// `levels` clamps to `2..=[MAX_POSTERIZE_LEVELS]` (256 = identity).
    Posterize {
        /// Band count (default 4).
        #[serde(default = "default_posterize_levels")]
        levels: u32,
    },
}

fn default_blur_radius() -> u32 {
    1
}

fn default_posterize_levels() -> u32 {
    4
}

impl FilterOp {
    /// Validates finite + clamps into range. Parse-time path: non-finite
    /// params are a typed error; out-of-range finite params clamp.
    fn normalized(self, filter_name: &str, op_index: usize) -> Result<Self, SmartError> {
        let non_finite = |param: &'static str| SmartError::NonFiniteParam {
            name: filter_name.to_string(),
            op: op_index,
            param,
        };
        Ok(match self {
            FilterOp::Blur { radius } => FilterOp::Blur {
                radius: radius.min(MAX_BLUR_RADIUS),
            },
            FilterOp::Levels { low, high, gamma } => {
                for (param, v) in [("low", low), ("high", high), ("gamma", gamma)] {
                    if !v.is_finite() {
                        return Err(non_finite(param));
                    }
                }
                FilterOp::Levels {
                    low: low.clamp(0.0, 1.0),
                    high: high.clamp(0.0, 1.0),
                    gamma: gamma.clamp(MIN_GAMMA, MAX_GAMMA),
                }
            }
            FilterOp::Curves { points } => {
                if points.len() > MAX_CURVE_POINTS {
                    return Err(SmartError::TooMany {
                        name: filter_name.to_string(),
                        what: "curve points",
                        count: points.len(),
                        cap: MAX_CURVE_POINTS,
                    });
                }
                let mut pts = Vec::with_capacity(points.len());
                for (x, y) in points {
                    if !x.is_finite() || !y.is_finite() {
                        return Err(non_finite("point"));
                    }
                    pts.push((x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)));
                }
                // Finite floats: partial_cmp is total here; the
                // fallback keeps the comparator total by construction.
                pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                FilterOp::Curves { points: pts }
            }
            FilterOp::Posterize { levels } => FilterOp::Posterize {
                levels: levels.clamp(2, MAX_POSTERIZE_LEVELS),
            },
        })
    }
}

/// An `.umberfilter` preset: a named, ordered list of [`FilterOp`]s
/// applied to a layer's RGBA8 buffer by [`SmartFilter::apply`].
///
/// File shape (serde JSON, extension `.umberfilter`):
///
/// ```json
/// {
///   "name": "Crunch",
///   "version": 1,
///   "ops": [
///     { "type": "levels", "low": 0.1, "high": 0.9, "gamma": 0.8 },
///     { "type": "posterize", "levels": 3 }
///   ]
/// }
/// ```
///
/// Determinism contract: `apply` is pure CPU with integer or LUT-based
/// float math — the same buffer + same params always produce
/// byte-identical output. There is no randomness, no threading, and no
/// platform-dependent step in any op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartFilter {
    /// Preset display name.
    pub name: String,
    /// Preset schema version (1 in this build).
    #[serde(default = "default_preset_version")]
    pub version: u32,
    /// Ops, applied bottom (index 0) first to top.
    pub ops: Vec<FilterOp>,
}

impl SmartFilter {
    /// Parses an `.umberfilter` document, validating every op (finite
    /// params, caps) and storing the clamped form.
    ///
    /// # Errors
    ///
    /// [`SmartError::Json`] on malformed input (including unknown op
    /// tags, `"NaN"` strings, over-deep nesting, and numbers too large
    /// for their integer type); [`SmartError::NonFiniteParam`] on float
    /// params that saturate to ±inf; [`SmartError::TooMany`] past the
    /// op / curve-point caps.
    pub fn from_json(json: &str) -> Result<Self, SmartError> {
        let filter: Self = serde_json::from_str(json)?;
        if filter.ops.len() > MAX_FILTER_OPS {
            return Err(SmartError::TooMany {
                name: filter.name,
                what: "ops",
                count: filter.ops.len(),
                cap: MAX_FILTER_OPS,
            });
        }
        let name = filter.name;
        let mut ops = Vec::with_capacity(filter.ops.len());
        for (i, op) in filter.ops.into_iter().enumerate() {
            ops.push(op.normalized(&name, i)?);
        }
        Ok(Self {
            name,
            version: filter.version,
            ops,
        })
    }

    /// Reads an `.umberfilter` file.
    ///
    /// # Errors
    ///
    /// [`SmartError::Io`] / [`SmartError::Json`] / [`SmartError::TooLarge`]
    /// on unreadable, oversized, or malformed input.
    pub fn load_file(path: &std::path::Path) -> Result<Self, SmartError> {
        let text = read_capped(path)?;
        Self::from_json(&text)
    }

    /// Serializes to pretty JSON (the on-disk `.umberfilter` form).
    ///
    /// # Errors
    ///
    /// [`SmartError::Json`] if serialization fails (only reachable for a
    /// struct built by hand with non-finite floats — `from_json` never
    /// stores one; serde would emit `null`, which fails to re-parse).
    pub fn to_json(&self) -> Result<String, SmartError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Writes the preset to `path` as pretty JSON, refusing to write
    /// more than [`MAX_PRESET_BYTES`].
    ///
    /// # Errors
    ///
    /// [`SmartError::TooLarge`] / [`SmartError::Io`] / [`SmartError::Json`].
    pub fn save_file(&self, path: &std::path::Path) -> Result<(), SmartError> {
        let text = self.to_json()?;
        if text.len() as u64 > MAX_PRESET_BYTES {
            return Err(SmartError::TooLarge(text.len() as u64));
        }
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Applies the op list, in order (index 0 first), to `pixels` —
    /// RGBA8, row-major, `width` × `height`. RGB channels are filtered;
    /// alpha is preserved by every op. Pure CPU and deterministic: the
    /// same buffer + same params always produce byte-identical output.
    ///
    /// # Errors
    ///
    /// [`SmartError::EmptyFilter`] when the preset has no ops;
    /// [`SmartError::BadBuffer`] when `pixels.len()` is not
    /// `width * height * 4` (overflow-safe: a hostile dimension pair
    /// saturates and fails the check, never panics).
    pub fn apply(&self, pixels: &mut [u8], width: usize, height: usize) -> Result<(), SmartError> {
        if self.ops.is_empty() {
            return Err(SmartError::EmptyFilter(self.name.clone()));
        }
        let expected = width.saturating_mul(height).saturating_mul(4);
        if pixels.len() != expected {
            return Err(SmartError::BadBuffer {
                expected,
                actual: pixels.len(),
            });
        }
        for op in &self.ops {
            op.apply_rgba(pixels, width, height);
        }
        Ok(())
    }
}

impl FilterOp {
    /// Applies this one op to the RGB channels of `buf` in place; alpha
    /// is never touched. Total: params that only a hand-built struct can
    /// make non-finite fall back to defaults here, never panic.
    fn apply_rgba(&self, buf: &mut [u8], width: usize, height: usize) {
        match self {
            FilterOp::Blur { radius } => {
                apply_blur(buf, width, height, (*radius).min(MAX_BLUR_RADIUS))
            }
            FilterOp::Levels { low, high, gamma } => {
                let lut = levels_lut(*low, *high, *gamma);
                apply_lut(buf, &lut);
            }
            FilterOp::Curves { points } => {
                let lut = curves_lut(points);
                apply_lut(buf, &lut);
            }
            FilterOp::Posterize { levels } => {
                let l = (*levels).clamp(2, MAX_POSTERIZE_LEVELS);
                for px in buf.chunks_exact_mut(4) {
                    for c in &mut px[..3] {
                        *c = posterize_channel(*c, l);
                    }
                }
            }
        }
    }
}

/// Maps every RGB channel through a 256-entry LUT; alpha untouched.
fn apply_lut(buf: &mut [u8], lut: &[u8; 256]) {
    for px in buf.chunks_exact_mut(4) {
        px[0] = lut[px[0] as usize];
        px[1] = lut[px[1] as usize];
        px[2] = lut[px[2] as usize];
    }
}

/// The levels LUT: `t = clamp01((v - low)/(high - low))`, `out = t^gamma`,
/// degenerate `high <= low` as a step at `low`. All inputs re-clamped
/// defensively; the result is always a finite `0..=255` round.
fn levels_lut(low: f32, high: f32, gamma: f32) -> [u8; 256] {
    let low = if low.is_finite() {
        low.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let high = if high.is_finite() {
        high.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let gamma = if gamma.is_finite() {
        gamma.clamp(MIN_GAMMA, MAX_GAMMA)
    } else {
        1.0
    };
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let v = i as f32 / 255.0;
        let out = if high <= low {
            if v <= low {
                0.0
            } else {
                1.0
            }
        } else {
            (((v - low) / (high - low)).clamp(0.0, 1.0))
                .powf(gamma)
                .clamp(0.0, 1.0)
        };
        *slot = (out * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    lut
}

/// The curves LUT: piecewise-linear interpolation between the (sorted,
/// clamped) control points, endpoints extended flat, empty = identity.
fn curves_lut(points: &[(f32, f32)]) -> [u8; 256] {
    let mut pts: Vec<(f32, f32)> = points
        .iter()
        .map(|&(x, y)| {
            (
                if x.is_finite() {
                    x.clamp(0.0, 1.0)
                } else {
                    0.0
                },
                if y.is_finite() {
                    y.clamp(0.0, 1.0)
                } else {
                    0.0
                },
            )
        })
        .collect();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let x = i as f32 / 255.0;
        let y = if pts.is_empty() {
            x
        } else if x <= pts[0].0 {
            pts[0].1
        } else if x >= pts[pts.len() - 1].0 {
            pts[pts.len() - 1].1
        } else {
            let mut j = 0;
            while j + 1 < pts.len() && pts[j + 1].0 <= x {
                j += 1;
            }
            let (x0, y0) = pts[j];
            let (x1, y1) = pts[j + 1];
            if x1 <= x0 {
                y1
            } else {
                y0 + (y1 - y0) * ((x - x0) / (x1 - x0))
            }
        };
        *slot = (y.clamp(0.0, 1.0) * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    lut
}

/// One posterize step, pure integer math:
/// `q = min(levels - 1, v * levels / 256)`, then
/// `out = round(q * 255 / (levels - 1))`. `levels >= 2` always (clamped
/// by the caller), so the divisor is never zero.
fn posterize_channel(v: u8, levels: u32) -> u8 {
    let q = ((v as u32 * levels) / 256).min(levels - 1);
    ((q * 255 + (levels - 1) / 2) / (levels - 1)) as u8
}

/// Separable box blur over RGB with border replication, integer
/// accumulation via per-line prefix sums (u64, so no overflow at any
/// sane width) — O(pixels), byte-identical for same input + radius.
/// Alpha is copied through both passes.
fn apply_blur(buf: &mut [u8], width: usize, height: usize, radius: u32) {
    let r = radius.min(MAX_BLUR_RADIUS) as usize;
    let Some(expected) = width.checked_mul(height).and_then(|n| n.checked_mul(4)) else {
        return;
    };
    if r == 0 || buf.len() != expected {
        return;
    }
    let mut tmp = vec![0u8; expected];
    let mut prefix = vec![[0u64; 3]; width.max(height) + 1];
    // Horizontal pass: buf -> tmp.
    for y in 0..height {
        let row = y * width;
        for x in 0..width {
            let i = (row + x) * 4;
            prefix[x + 1][0] = prefix[x][0] + buf[i] as u64;
            prefix[x + 1][1] = prefix[x][1] + buf[i + 1] as u64;
            prefix[x + 1][2] = prefix[x][2] + buf[i + 2] as u64;
        }
        for x in 0..width {
            let x0 = x.saturating_sub(r);
            let x1 = (x + r).min(width - 1);
            let count = (x1 - x0 + 1) as u64;
            let o = (row + x) * 4;
            tmp[o] = ((prefix[x1 + 1][0] - prefix[x0][0]) / count) as u8;
            tmp[o + 1] = ((prefix[x1 + 1][1] - prefix[x0][1]) / count) as u8;
            tmp[o + 2] = ((prefix[x1 + 1][2] - prefix[x0][2]) / count) as u8;
            tmp[o + 3] = buf[o + 3];
        }
    }
    // Vertical pass: tmp -> buf.
    for x in 0..width {
        for y in 0..height {
            let i = (y * width + x) * 4;
            prefix[y + 1][0] = prefix[y][0] + tmp[i] as u64;
            prefix[y + 1][1] = prefix[y][1] + tmp[i + 1] as u64;
            prefix[y + 1][2] = prefix[y][2] + tmp[i + 2] as u64;
        }
        for y in 0..height {
            let y0 = y.saturating_sub(r);
            let y1 = (y + r).min(height - 1);
            let count = (y1 - y0 + 1) as u64;
            let o = (y * width + x) * 4;
            buf[o] = ((prefix[y1 + 1][0] - prefix[y0][0]) / count) as u8;
            buf[o + 1] = ((prefix[y1 + 1][1] - prefix[y0][1]) / count) as u8;
            buf[o + 2] = ((prefix[y1 + 1][2] - prefix[y0][2]) / count) as u8;
        }
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
            parameters: vec![],
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

    // ---- 4.1a smart-material parameters (2026-11-10) ----------------------

    #[test]
    fn old_format_preset_parses_with_empty_parameters() {
        // WORN_METAL_JSON predates the `parameters` key (and its bindings
        // predate `param`): the block defaults to empty and is NOT invented
        // on save, which is what keeps every other shipped starter
        // byte-identical under the round-trip pin.
        let preset = SmartMaterial::from_json(WORN_METAL_JSON).expect("old format parses");
        assert!(preset.parameters.is_empty(), "absent block = empty vec");
        let saved = serde_json::to_string_pretty(&preset).expect("serializes");
        assert!(
            !saved.contains("parameters"),
            "an empty block must not be invented on save: {saved}"
        );
        assert!(
            !saved.contains("\"param\""),
            "a fixed generator must not gain a param key: {saved}"
        );
    }

    #[test]
    fn param_references_resolve_ok_and_fail_loud() {
        let material = SmartMaterial::load_file(&starter_dir().join("rusted-iron.umbersmart"))
            .expect("shipped rusted iron loads");
        assert_eq!(
            material.param("rust-coverage").expect("declared").value(),
            SmartParamValue::Float(0.55)
        );
        assert_eq!(
            material.param("oxide-mode").expect("declared").value(),
            SmartParamValue::Choice("metal")
        );
        let rust = material
            .layers
            .iter()
            .find(|layer| layer.name == "Rust patches")
            .expect("layer present");
        assert_eq!(
            material
                .resolve_binding(rust)
                .expect("binding resolves")
                .map(|param| param.id.as_str()),
            Some("rust-coverage")
        );
        // A folder (no generator) and a fixed binding both resolve to None.
        let folder = material
            .layers
            .iter()
            .find(|layer| layer.name == "Iron folder")
            .expect("layer present");
        assert!(material
            .resolve_binding(folder)
            .expect("no generator is not an error")
            .is_none());
        let edge = material
            .layers
            .iter()
            .find(|layer| layer.name == "Edge rust")
            .expect("layer present");
        assert!(material
            .resolve_binding(edge)
            .expect("fixed binding is not an error")
            .is_none());
        // Unknown lookup and unknown reference are typed errors.
        assert!(matches!(
            material.param("ghost"),
            Err(SmartError::UnknownParamId { .. })
        ));
        let mut dangling = rust.clone();
        dangling.generator.as_mut().expect("has a generator").param = Some("ghost".into());
        match material.resolve_binding(&dangling) {
            Err(SmartError::UnknownParam { layer, param, .. }) => {
                assert_eq!(layer, "Rust patches");
                assert_eq!(param, "ghost");
            }
            other => panic!("dangling param ref must be typed, got {other:?}"),
        }
    }

    #[test]
    fn unknown_param_reference_fails_parse_and_apply_without_touching_the_stack() {
        let json = r#"{
            "name": "Dangling",
            "layers": [{ "name": "Wear", "kind": "fill",
                         "generator": { "node_def": "noise_perlin", "param": "ghost" } }],
            "parameters": []
        }"#;
        match SmartMaterial::from_json(json) {
            Err(SmartError::UnknownParam { layer, param, .. }) => {
                assert_eq!(layer, "Wear");
                assert_eq!(param, "ghost");
            }
            other => panic!("parse must refuse the dangling ref, got {other:?}"),
        }
        // The same preset built by hand (serde bypassed) is refused at
        // apply, before the first layer is added.
        let preset = SmartMaterial {
            name: "Dangling".into(),
            version: 1,
            layers: vec![SmartLayerTemplate {
                name: "Wear".into(),
                kind: SmartLayerKind::Fill,
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                mask: None,
                generator: Some(GeneratorBinding {
                    node_def: "noise_perlin".into(),
                    map_name: String::new(),
                    channel: "baseColor".into(),
                    param: Some("ghost".into()),
                }),
            }],
            parameters: vec![],
        };
        let mut stack = LayerStack::new();
        assert!(matches!(
            preset.apply(&mut stack),
            Err(SmartError::UnknownParam { .. })
        ));
        assert!(
            stack.layers.is_empty(),
            "a refused apply must not half-apply the template"
        );
    }

    fn material_with_param(param_json: &str) -> Result<SmartMaterial, SmartError> {
        SmartMaterial::from_json(&format!(
            r#"{{ "name": "P", "layers": [{{ "name": "L", "kind": "fill" }}],
                 "parameters": [{param_json}] }}"#
        ))
    }

    #[test]
    fn malformed_parameter_declarations_are_typed_parse_errors() {
        let cases = [
            (
                r#"{ "id": "a", "label": "A", "kind": { "type": "float", "min": 0.0, "max": 1.0, "step": 0.01, "default": 2.0 } }"#,
                "default is outside min..=max",
            ),
            (
                r#"{ "id": "a", "label": "A", "kind": { "type": "float", "min": 2.0, "max": 1.0, "step": 0.01, "default": 1.5 } }"#,
                "min is greater than max",
            ),
            (
                r#"{ "id": "a", "label": "A", "kind": { "type": "float", "min": 0.0, "max": 1.0, "step": 0.0, "default": 0.5 } }"#,
                "step must be positive",
            ),
            (
                r#"{ "id": "a", "label": "A", "kind": { "type": "float", "min": 0.0, "max": 1.0, "step": 0.01, "default": 1e300 } }"#,
                "non-finite float bound",
            ),
            (
                r#"{ "id": "a", "label": "A", "kind": { "type": "choice", "options": ["x", "y"], "default": "z" } }"#,
                "default is not one of the options",
            ),
            (
                r#"{ "id": "a", "label": "A", "kind": { "type": "choice", "options": [], "default": "z" } }"#,
                "choice has no options",
            ),
            (
                r#"{ "id": "a", "label": "A", "kind": { "type": "choice", "options": ["x", "x"], "default": "x" } }"#,
                "duplicate choice option",
            ),
            (
                r#"{ "id": " ", "label": "A", "kind": { "type": "float", "min": 0.0, "max": 1.0, "step": 0.01, "default": 0.5 } }"#,
                "empty parameter id",
            ),
            (
                r#"{ "id": "a", "label": "", "kind": { "type": "float", "min": 0.0, "max": 1.0, "step": 0.01, "default": 0.5 } }"#,
                "empty parameter label",
            ),
        ];
        for (json, why) in cases {
            match material_with_param(json) {
                Err(SmartError::BadParam { why: got, .. }) => assert_eq!(got, why, "{json}"),
                other => panic!("{json} must be a typed BadParam, got {other:?}"),
            }
        }
        // A `type` outside the closed vocabulary fails the parse itself.
        assert!(matches!(
            material_with_param(
                r#"{ "id": "a", "label": "A", "kind": { "type": "int", "default": 1 } }"#
            ),
            Err(SmartError::Json(_))
        ));
        // Duplicate ids are structural (each declaration is valid).
        let duplicate = r#"{ "name": "D", "layers": [{ "name": "L" }], "parameters": [
            { "id": "a", "label": "A", "kind": { "type": "float", "min": 0.0, "max": 1.0, "step": 0.1, "default": 0.5 } },
            { "id": "a", "label": "B", "kind": { "type": "float", "min": 0.0, "max": 1.0, "step": 0.1, "default": 0.5 } }
        ] }"#;
        match SmartMaterial::from_json(duplicate) {
            Err(SmartError::BadParam {
                material,
                param,
                why,
            }) => {
                assert_eq!(material, "D");
                assert_eq!(param, "a");
                assert_eq!(why, "duplicate parameter id");
            }
            other => panic!("duplicate ids must be a typed BadParam, got {other:?}"),
        }
    }

    #[test]
    fn parameter_and_choice_caps_are_enforced() {
        let param = r#"{ "id": "ID", "label": "L", "kind": { "type": "float", "min": 0.0, "max": 1.0, "step": 0.1, "default": 0.5 } }"#;
        let params: Vec<String> = (0..=MAX_PRESET_PARAMS)
            .map(|i| param.replace("ID", &format!("p{i}")))
            .collect();
        let json = format!(
            r#"{{ "name": "Big", "layers": [{{ "name": "L" }}], "parameters": [{}] }}"#,
            params.join(",")
        );
        match SmartMaterial::from_json(&json) {
            Err(SmartError::TooMany {
                name,
                what: "parameters",
                count,
                cap,
            }) => {
                assert_eq!(name, "Big");
                assert_eq!(count, MAX_PRESET_PARAMS + 1);
                assert_eq!(cap, MAX_PRESET_PARAMS);
            }
            other => panic!("over the parameter cap must be a typed TooMany, got {other:?}"),
        }
        let options: Vec<String> = (0..=MAX_PARAM_CHOICES)
            .map(|i| format!("\"o{i}\""))
            .collect();
        let json = format!(
            r#"{{ "name": "BigChoice", "layers": [{{ "name": "L" }}], "parameters": [
                {{ "id": "c", "label": "C", "kind": {{ "type": "choice", "options": [{}], "default": "o0" }} }}
            ] }}"#,
            options.join(",")
        );
        match SmartMaterial::from_json(&json) {
            Err(SmartError::TooMany {
                name,
                what: "choices",
                count,
                cap,
            }) => {
                assert_eq!(name, "BigChoice");
                assert_eq!(count, MAX_PARAM_CHOICES + 1);
                assert_eq!(cap, MAX_PARAM_CHOICES);
            }
            other => panic!("over the choice cap must be a typed TooMany, got {other:?}"),
        }
    }

    #[test]
    fn shipped_param_materials_are_wired_not_decorative() {
        // Every declared parameter must drive at least one generator
        // binding, and every binding ref must resolve — a decorative
        // declaration fails the first check, a dangling ref the second.
        for file in ["aged-leather.umbersmart", "rusted-iron.umbersmart"] {
            let material = SmartMaterial::load_file(&starter_dir().join(file)).expect("loads");
            assert!(
                !material.parameters.is_empty(),
                "{file} declares parameters"
            );
            let mut referenced = std::collections::BTreeSet::new();
            for layer in &material.layers {
                if let Some(param) = material.resolve_binding(layer).expect("ref resolves") {
                    referenced.insert(param.id.as_str());
                }
            }
            let declared: std::collections::BTreeSet<&str> = material
                .parameters
                .iter()
                .map(|param| param.id.as_str())
                .collect();
            assert_eq!(referenced, declared, "{file}: declared == referenced");
        }
        // Pinned authoring: the two locked materials ship these controls.
        let leather = SmartMaterial::load_file(&starter_dir().join("aged-leather.umbersmart"))
            .expect("leather loads");
        assert_eq!(leather.parameters.len(), 2);
        assert_eq!(
            leather.param("wear").expect("wear").value(),
            SmartParamValue::Float(0.35)
        );
        assert_eq!(
            leather.param("roughness").expect("roughness").value(),
            SmartParamValue::Float(0.6)
        );
        let iron = SmartMaterial::load_file(&starter_dir().join("rusted-iron.umbersmart"))
            .expect("iron loads");
        assert_eq!(iron.parameters.len(), 2);
        assert_eq!(
            iron.param("rust-coverage").expect("coverage").value(),
            SmartParamValue::Float(0.55)
        );
        assert_eq!(
            iron.param("oxide-mode").expect("mode").value(),
            SmartParamValue::Choice("metal")
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

    const CRUNCH_FILTER_JSON: &str = r#"{
        "name": "Crunch",
        "version": 1,
        "ops": [
            { "type": "levels", "low": 0.1, "high": 0.9, "gamma": 0.8 },
            { "type": "posterize", "levels": 3 },
            { "type": "blur", "radius": 2 },
            { "type": "curves", "points": [[0.0, 0.2], [1.0, 0.8]] }
        ]
    }"#;

    #[test]
    fn smart_filter_save_load_roundtrip_is_exact() {
        let filter = SmartFilter::from_json(CRUNCH_FILTER_JSON).expect("parses");
        let path = std::env::temp_dir().join(format!(
            "umber-filter-rt-{}.umberfilter",
            std::process::id()
        ));
        filter.save_file(&path).expect("saves");
        let loaded = SmartFilter::load_file(&path).expect("loads");
        assert_eq!(loaded, filter, "save -> load must be struct-exact");
        assert_eq!(
            loaded.to_json().unwrap(),
            filter.to_json().unwrap(),
            "re-serializing the loaded preset is byte-identical"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn smart_kind_for_path_detects_all_three_extensions() {
        assert_eq!(
            smart_kind_for_path(std::path::Path::new("a.umbersmart")),
            Some(SmartAssetKind::Material)
        );
        assert_eq!(
            smart_kind_for_path(std::path::Path::new("b.UMBERMASK")),
            Some(SmartAssetKind::Mask)
        );
        assert_eq!(
            smart_kind_for_path(std::path::Path::new("c.UmberFilter")),
            Some(SmartAssetKind::Filter)
        );
        assert_eq!(smart_kind_for_path(std::path::Path::new("d.png")), None);
        assert_eq!(smart_kind_for_path(std::path::Path::new("no-ext")), None);
        assert_eq!(SMART_FILTER_EXT, "umberfilter");
    }

    #[test]
    fn filter_malformed_and_hostile_inputs_are_typed_errors() {
        // Garbage.
        assert!(matches!(
            SmartFilter::from_json("not json"),
            Err(SmartError::Json(_))
        ));
        // Unknown op tag: closed vocabulary, loud parse failure.
        let unknown = r#"{"name":"x","ops":[{"type":"sharpen","amount":9}]}"#;
        assert!(matches!(
            SmartFilter::from_json(unknown),
            Err(SmartError::Json(_))
        ));
        // NaN as a string fails the f32 parse.
        let nan_str = r#"{"name":"x","ops":[{"type":"levels","gamma":"NaN"}]}"#;
        assert!(matches!(
            SmartFilter::from_json(nan_str),
            Err(SmartError::Json(_))
        ));
        // A huge float saturates f32 to +inf: typed, never stored.
        let huge = r#"{"name":"x","ops":[{"type":"levels","gamma":1e300}]}"#;
        assert!(matches!(
            SmartFilter::from_json(huge),
            Err(SmartError::NonFiniteParam {
                op: 0,
                param: "gamma",
                ..
            })
        ));
        // An integer param beyond its type's range fails the parse.
        let huge_int = r#"{"name":"x","ops":[{"type":"posterize","levels":99999999999999999999}]}"#;
        assert!(matches!(
            SmartFilter::from_json(huge_int),
            Err(SmartError::Json(_))
        ));
        // Deep nesting: serde_json's recursion limit errors out, no
        // stack overflow.
        let deep = format!("{{\"name\":\"x\",\"ops\":{} }}", "[".repeat(10_000));
        assert!(matches!(
            SmartFilter::from_json(&deep),
            Err(SmartError::Json(_))
        ));
        // Ops over the cap.
        let op = r#"{"type":"posterize","levels":2}"#;
        let ops = vec![op; MAX_FILTER_OPS + 1].join(",");
        assert!(matches!(
            SmartFilter::from_json(&format!(r#"{{"name":"Big","ops":[{ops}]}}"#)),
            Err(SmartError::TooMany { what: "ops", .. })
        ));
        // Curve points over the cap.
        let pts = vec!["[0.5,0.5]"; MAX_CURVE_POINTS + 1].join(",");
        assert!(matches!(
            SmartFilter::from_json(&format!(
                r#"{{"name":"C","ops":[{{"type":"curves","points":[{pts}]}}]}}"#
            )),
            Err(SmartError::TooMany {
                what: "curve points",
                ..
            })
        ));
        // An oversized file is refused from metadata, unread.
        let path = std::env::temp_dir().join(format!(
            "umber-big-filter-{}.umberfilter",
            std::process::id()
        ));
        std::fs::write(&path, vec![b' '; (MAX_PRESET_BYTES + 1) as usize]).unwrap();
        assert!(matches!(
            SmartFilter::load_file(&path),
            Err(SmartError::TooLarge(_))
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn filter_params_are_clamped_at_parse() {
        let json = r#"{"name":"clamp","ops":[
            {"type":"blur","radius":4000},
            {"type":"posterize","levels":999999},
            {"type":"posterize","levels":0},
            {"type":"levels","low":-5.0,"high":9.0,"gamma":100.0},
            {"type":"curves","points":[[5.0,-3.0],[0.5,0.5]]}
        ]}"#;
        let f = SmartFilter::from_json(json).expect("finite out-of-range params clamp, not error");
        assert_eq!(
            f.ops[0],
            FilterOp::Blur {
                radius: MAX_BLUR_RADIUS
            }
        );
        assert_eq!(
            f.ops[1],
            FilterOp::Posterize {
                levels: MAX_POSTERIZE_LEVELS
            }
        );
        assert_eq!(f.ops[2], FilterOp::Posterize { levels: 2 });
        assert_eq!(
            f.ops[3],
            FilterOp::Levels {
                low: 0.0,
                high: 1.0,
                gamma: MAX_GAMMA
            }
        );
        // Curve points clamp into 0..=1 and sort by x.
        assert_eq!(
            f.ops[4],
            FilterOp::Curves {
                points: vec![(0.5, 0.5), (1.0, 0.0)]
            }
        );
    }

    #[test]
    fn posterize_two_levels_pins_a_known_2x2_answer() {
        // 2x2 RGBA; R=G=B per pixel are 0, 64, 128, 255; alpha distinct.
        let src: [u8; 16] = [
            0, 0, 0, 255, //
            64, 64, 64, 200, //
            128, 128, 128, 100, //
            255, 255, 255, 0,
        ];
        let filter =
            SmartFilter::from_json(r#"{"name":"p2","ops":[{"type":"posterize","levels":2}]}"#)
                .unwrap();
        let mut buf = src;
        filter.apply(&mut buf, 2, 2).expect("applies");
        // q = min(1, v*2/256): 0->0, 64->0, 128->1, 255->1; out = q*255.
        // Alpha passes through untouched.
        let expected: [u8; 16] = [
            0, 0, 0, 255, //
            0, 0, 0, 200, //
            255, 255, 255, 100, //
            255, 255, 255, 0,
        ];
        assert_eq!(buf, expected, "posterize levels=2 known answer");
    }

    #[test]
    fn blur_radius_one_pins_a_known_row_answer() {
        // 3x1 row of gray values 0, 90, 180, radius 1, border-replicated:
        // windows [0,1], [0,2], [1,2] -> means 45, 90, 135 (the vertical
        // pass is a no-op on a 1px-tall image).
        let src: [u8; 12] = [
            0, 0, 0, 255, //
            90, 90, 90, 200, //
            180, 180, 180, 100,
        ];
        let filter =
            SmartFilter::from_json(r#"{"name":"b","ops":[{"type":"blur","radius":1}]}"#).unwrap();
        let mut buf = src;
        filter.apply(&mut buf, 3, 1).unwrap();
        let expected: [u8; 12] = [
            45, 45, 45, 255, //
            90, 90, 90, 200, //
            135, 135, 135, 100,
        ];
        assert_eq!(buf, expected, "blur radius=1 known answer");
    }

    #[test]
    fn levels_degenerate_range_is_a_step_not_a_div_by_zero() {
        let filter = SmartFilter::from_json(
            r#"{"name":"s","ops":[{"type":"levels","low":0.5,"high":0.5,"gamma":1.0}]}"#,
        )
        .unwrap();
        let mut buf: [u8; 8] = [0, 0, 0, 255, 200, 200, 200, 255];
        filter.apply(&mut buf, 2, 1).unwrap();
        // v=0 <= 0.5 -> 0; v=200/255 ~= 0.784 > 0.5 -> 255.
        assert_eq!(&buf[..4], &[0, 0, 0, 255]);
        assert_eq!(&buf[4..], &[255, 255, 255, 255]);
    }

    #[test]
    fn filter_apply_is_deterministic_byte_identical() {
        let filter = SmartFilter::from_json(CRUNCH_FILTER_JSON).unwrap();
        // Deterministic pseudo-random 8x8 buffer (LCG; no rand crate).
        let mut src = [0u8; 8 * 8 * 4];
        let mut state = 0x1234_5678u32;
        for b in src.iter_mut() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *b = (state >> 24) as u8;
        }
        let mut a = src;
        let mut b = src;
        filter.apply(&mut a, 8, 8).unwrap();
        filter.apply(&mut b, 8, 8).unwrap();
        assert_eq!(a, b, "same input + params must be byte-identical");
        assert_ne!(a, src, "the op chain actually changed pixels");
        for (i, px) in a.chunks_exact(4).enumerate() {
            assert_eq!(px[3], src[i * 4 + 3], "alpha must pass through untouched");
        }
    }

    #[test]
    fn filter_apply_rejects_empty_and_bad_buffers_typed() {
        let empty = SmartFilter {
            name: "e".into(),
            version: 1,
            ops: vec![],
        };
        let mut buf = [0u8; 4];
        assert!(matches!(
            empty.apply(&mut buf, 1, 1),
            Err(SmartError::EmptyFilter(_))
        ));
        let f =
            SmartFilter::from_json(r#"{"name":"p","ops":[{"type":"blur","radius":1}]}"#).unwrap();
        assert!(matches!(
            f.apply(&mut buf, 2, 2),
            Err(SmartError::BadBuffer {
                expected: 16,
                actual: 4
            })
        ));
        // Hostile dimensions saturate instead of panicking.
        assert!(matches!(
            f.apply(&mut buf, usize::MAX, usize::MAX),
            Err(SmartError::BadBuffer { .. })
        ));
    }

    // ---- K7 starter-library round-trip pins (assets wave) ------------------
    //
    // Each .umbersmart / .umbermask this wave ships under assets/smart/ (8
    // materials + 4 masks) is pinned: load_file -> canonical save (pretty
    // JSON + trailing newline, the on-disk form of every starter file) must
    // come back byte-identical to the committed file, so the starter library
    // can never silently drift from the serializer. One pin per file.

    fn starter_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/smart")
    }

    fn roundtrip_pin<T: serde::Serialize>(
        file: &str,
        load: fn(&std::path::Path) -> Result<T, SmartError>,
    ) {
        let path = starter_dir().join(file);
        let original = std::fs::read(&path).expect("shipped starter file is present");
        let preset = load(&path).expect("shipped starter file loads");
        let mut saved = serde_json::to_string_pretty(&preset).expect("preset serializes");
        saved.push('\n');
        let tmp =
            std::env::temp_dir().join(format!("umber-starter-rt-{}-{}", std::process::id(), file));
        std::fs::write(&tmp, saved.as_bytes()).expect("save writes");
        let read_back = std::fs::read(&tmp).expect("saved file reads back");
        let _ = std::fs::remove_file(&tmp);
        assert_eq!(
            read_back, original,
            "{file}: load_file -> save -> byte-identical pin"
        );
    }

    #[test]
    fn starter_aged_leather_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("aged-leather.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter_brushed_steel_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("brushed-steel.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter_glazed_ceramic_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("glazed-ceramic.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter_matte_plastic_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("matte-plastic.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter_polished_marble_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("polished-marble.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter_raw_concrete_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("raw-concrete.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter_rusted_iron_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("rusted-iron.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter_woven_fabric_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("woven-fabric.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter_ao_cavity_umbermask_roundtrip_byte_identical() {
        roundtrip_pin("ao-cavity.umbermask", SmartMask::load_file);
    }

    #[test]
    fn starter_grime_noise_umbermask_roundtrip_byte_identical() {
        roundtrip_pin("grime-noise.umbermask", SmartMask::load_file);
    }

    #[test]
    fn starter_height_ridges_umbermask_roundtrip_byte_identical() {
        roundtrip_pin("height-ridges.umbermask", SmartMask::load_file);
    }

    #[test]
    fn starter_inverted_wear_umbermask_roundtrip_byte_identical() {
        roundtrip_pin("inverted-wear.umbermask", SmartMask::load_file);
    }

    // Starter wave 2 (assets lane, 2026-11-06): +10 materials, +6 masks,
    // pinned by the same load_file -> save byte-identical contract.

    #[test]
    fn starter2_oxidized_copper_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("oxidized-copper.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter2_weathered_bronze_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("weathered-bronze.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter2_cast_aluminum_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("cast-aluminum.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter2_chipped_paint_metal_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("chipped-paint-metal.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter2_carbon_fiber_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("carbon-fiber.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter2_rubber_worn_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("rubber-worn.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter2_sandstone_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("sandstone.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter2_mossy_stone_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("mossy-stone.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter2_lacquered_wood_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("lacquered-wood.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter2_frosted_glass_umbersmart_roundtrip_byte_identical() {
        roundtrip_pin("frosted-glass.umbersmart", SmartMaterial::load_file);
    }

    #[test]
    fn starter2_dust_top_umbermask_roundtrip_byte_identical() {
        roundtrip_pin("dust-top.umbermask", SmartMask::load_file);
    }

    #[test]
    fn starter2_drip_streaks_umbermask_roundtrip_byte_identical() {
        roundtrip_pin("drip-streaks.umbermask", SmartMask::load_file);
    }

    #[test]
    fn starter2_scratches_umbermask_roundtrip_byte_identical() {
        roundtrip_pin("scratches.umbermask", SmartMask::load_file);
    }

    #[test]
    fn starter2_cavity_dirt_umbermask_roundtrip_byte_identical() {
        roundtrip_pin("cavity-dirt.umbermask", SmartMask::load_file);
    }

    #[test]
    fn starter2_moss_crevices_umbermask_roundtrip_byte_identical() {
        roundtrip_pin("moss-crevices.umbermask", SmartMask::load_file);
    }

    #[test]
    fn starter2_height_peaks_umbermask_roundtrip_byte_identical() {
        roundtrip_pin("height-peaks.umbermask", SmartMask::load_file);
    }

    // ---- SmartFilter::apply reference battery -----------------------------
    //
    // Goldens below are computed by independent reference code in the test
    // (naive windowed blur, plain integer posterize) or by hand, never by
    // calling the production LUT / prefix-sum helpers.

    fn one_op(op: FilterOp) -> SmartFilter {
        SmartFilter {
            name: "ref".into(),
            version: 1,
            ops: vec![op],
        }
    }

    fn lcg_rgba(w: usize, h: usize, seed: u32) -> Vec<u8> {
        let mut state = seed;
        (0..w * h * 4)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect()
    }

    fn reference_ops() -> Vec<FilterOp> {
        vec![
            FilterOp::Blur { radius: 2 },
            FilterOp::Levels {
                low: 0.1,
                high: 0.8,
                gamma: 0.7,
            },
            FilterOp::Curves {
                points: vec![(0.2, 0.1), (0.5, 0.7), (0.9, 0.95)],
            },
            FilterOp::Posterize { levels: 3 },
        ]
    }

    #[test]
    fn every_op_is_deterministic_on_a_seeded_64x64_buffer() {
        let src = lcg_rgba(64, 64, 0xC0FF_EE11);
        for op in reference_ops() {
            let f = one_op(op.clone());
            let mut a = src.clone();
            let mut b = src.clone();
            f.apply(&mut a, 64, 64).unwrap();
            f.apply(&mut b, 64, 64).unwrap();
            assert_eq!(a, b, "{op:?} must be byte-identical across runs");
            assert_ne!(a, src, "{op:?} actually changed the buffer");
            for (o, s) in a.chunks_exact(4).zip(src.chunks_exact(4)) {
                assert_eq!(o[3], s[3], "{op:?} preserves alpha");
            }
        }
    }

    #[test]
    fn blur_and_posterize_match_independent_reference_on_4x4() {
        let src = lcg_rgba(4, 4, 7);
        let (w, h) = (4usize, 4usize);
        // Naive separable box blur, radius 1, border-replicated (window
        // clipped to the image), truncating integer mean per pass.
        let r = 1usize;
        let mut tmp = src.clone();
        for y in 0..h {
            for x in 0..w {
                for c in 0..3 {
                    let lo = x.saturating_sub(r);
                    let hi = (x + r).min(w - 1);
                    let sum: u32 = (lo..=hi).map(|xx| src[(y * w + xx) * 4 + c] as u32).sum();
                    tmp[(y * w + x) * 4 + c] = (sum / (hi - lo + 1) as u32) as u8;
                }
            }
        }
        let mut want = tmp.clone();
        for y in 0..h {
            for x in 0..w {
                for c in 0..3 {
                    let lo = y.saturating_sub(r);
                    let hi = (y + r).min(h - 1);
                    let sum: u32 = (lo..=hi).map(|yy| tmp[(yy * w + x) * 4 + c] as u32).sum();
                    want[(y * w + x) * 4 + c] = (sum / (hi - lo + 1) as u32) as u8;
                }
            }
        }
        let mut got = src.clone();
        one_op(FilterOp::Blur { radius: 1 })
            .apply(&mut got, w, h)
            .unwrap();
        assert_eq!(got, want, "blur r=1 vs naive reference");

        // Posterize: band = v*L/256 (capped), out = round(band*255/(L-1)).
        for levels in [2u32, 3, 5, 256] {
            let mut want = src.clone();
            for px in want.chunks_exact_mut(4) {
                for c in &mut px[..3] {
                    let band = (*c as u32 * levels / 256).min(levels - 1);
                    let num = band * 255 * 2 + (levels - 1);
                    *c = (num / (2 * (levels - 1))) as u8;
                }
            }
            let mut got = src.clone();
            one_op(FilterOp::Posterize { levels })
                .apply(&mut got, w, h)
                .unwrap();
            assert_eq!(got, want, "posterize levels={levels} vs reference");
        }
        // levels=256 is the identity.
        let mut id = src.clone();
        one_op(FilterOp::Posterize { levels: 256 })
            .apply(&mut id, w, h)
            .unwrap();
        assert_eq!(id, src);
    }

    #[test]
    fn levels_and_curves_pin_hand_computed_values() {
        // One gray channel value per pixel (R=G=B), alpha 255.
        let gray = |vals: &[u8]| -> Vec<u8> { vals.iter().flat_map(|&v| [v, v, v, 255]).collect() };
        let run = |op: FilterOp, vals: &[u8]| -> Vec<u8> {
            let mut buf = gray(vals);
            one_op(op).apply(&mut buf, vals.len(), 1).unwrap();
            buf.chunks_exact(4).map(|p| p[0]).collect()
        };
        // low=0, high=0.5, gamma=1: out = min(1, 2v/255)*255.
        // 51 -> 0.4 -> 102; 100 -> 200/255 -> 200; 128 -> >1 -> 255.
        let lv = FilterOp::Levels {
            low: 0.0,
            high: 0.5,
            gamma: 1.0,
        };
        assert_eq!(
            run(lv, &[0, 51, 100, 128, 255]),
            vec![0, 102, 200, 255, 255]
        );
        // gamma=2 on the full range: 51 -> 0.2^2*255 = 10.2 -> 10.
        let g2 = FilterOp::Levels {
            low: 0.0,
            high: 1.0,
            gamma: 2.0,
        };
        assert_eq!(run(g2, &[0, 51, 255]), vec![0, 10, 255]);
        // Curves (0.25,0.2)-(0.75,0.8): flat endpoints at 51 / 204;
        // 128 -> t = (128/255 - 0.25)/0.5 ~= 0.5039 -> 0.2 + 0.6t ~= 0.5024
        // -> 128.1 -> 128.
        let cv = FilterOp::Curves {
            points: vec![(0.25, 0.2), (0.75, 0.8)],
        };
        assert_eq!(
            run(cv, &[0, 10, 128, 250, 255]),
            vec![51, 51, 128, 204, 204]
        );
        // Empty curve = identity.
        let vals: Vec<u8> = (0..=255u8).step_by(17).collect();
        assert_eq!(run(FilterOp::Curves { points: vec![] }, &vals), vals);
    }

    #[test]
    fn filter_edge_semantics_are_pinned() {
        // 1x1: every op succeeds; blur on a single pixel is the identity.
        for op in reference_ops() {
            let mut px = [37u8, 140, 222, 9];
            one_op(op.clone()).apply(&mut px, 1, 1).unwrap();
            assert_eq!(px[3], 9, "{op:?} 1x1 alpha");
        }
        let mut px = [37u8, 140, 222, 9];
        one_op(FilterOp::Blur { radius: 5 })
            .apply(&mut px, 1, 1)
            .unwrap();
        assert_eq!(px, [37, 140, 222, 9]);

        // Alpha-0 pin: apply is straight (non-premultiplied) — RGB under
        // alpha 0 is still filtered, and alpha stays exactly 0.
        let mut clear = [0u8, 64, 128, 0, 255, 200, 100, 0];
        one_op(FilterOp::Posterize { levels: 2 })
            .apply(&mut clear, 2, 1)
            .unwrap();
        assert_eq!(clear, [0, 0, 255, 0, 255, 255, 0, 0]);
        let mut clear = [0u8, 0, 0, 0, 90, 90, 90, 0];
        one_op(FilterOp::Blur { radius: 1 })
            .apply(&mut clear, 2, 1)
            .unwrap();
        assert_eq!(clear, [45, 45, 45, 0, 45, 45, 45, 0]);

        // Zero-size: a 0xN / Nx0 image with an empty buffer is a valid
        // no-op Ok; any non-empty buffer against zero dims is BadBuffer.
        for (w, h) in [(0usize, 0usize), (0, 5), (5, 0)] {
            for op in reference_ops() {
                let mut empty: [u8; 0] = [];
                assert!(one_op(op).apply(&mut empty, w, h).is_ok());
            }
            let mut stray = [1u8; 4];
            assert!(matches!(
                one_op(FilterOp::Blur { radius: 1 }).apply(&mut stray, w, h),
                Err(SmartError::BadBuffer {
                    expected: 0,
                    actual: 4
                })
            ));
        }
    }

    #[test]
    fn hand_built_out_of_range_params_clamp_at_apply() {
        // apply() re-clamps params a hand-built struct can put out of
        // range: each extreme must equal its clamped twin byte-for-byte.
        let src = lcg_rgba(16, 16, 99);
        let pairs = [
            (
                FilterOp::Posterize { levels: 0 },
                FilterOp::Posterize { levels: 2 },
            ),
            (
                FilterOp::Posterize { levels: u32::MAX },
                FilterOp::Posterize {
                    levels: MAX_POSTERIZE_LEVELS,
                },
            ),
            (
                FilterOp::Blur { radius: u32::MAX },
                FilterOp::Blur {
                    radius: MAX_BLUR_RADIUS,
                },
            ),
            (
                FilterOp::Levels {
                    low: -3.0,
                    high: 7.0,
                    gamma: 1e9,
                },
                FilterOp::Levels {
                    low: 0.0,
                    high: 1.0,
                    gamma: MAX_GAMMA,
                },
            ),
            (
                FilterOp::Levels {
                    low: 0.0,
                    high: 1.0,
                    gamma: 0.0,
                },
                FilterOp::Levels {
                    low: 0.0,
                    high: 1.0,
                    gamma: MIN_GAMMA,
                },
            ),
            (
                FilterOp::Levels {
                    low: f32::NAN,
                    high: f32::INFINITY,
                    gamma: f32::NAN,
                },
                FilterOp::Levels {
                    low: 0.0,
                    high: 1.0,
                    gamma: 1.0,
                },
            ),
        ];
        for (wild, tame) in pairs {
            let mut a = src.clone();
            let mut b = src.clone();
            one_op(wild.clone()).apply(&mut a, 16, 16).unwrap();
            one_op(tame).apply(&mut b, 16, 16).unwrap();
            assert_eq!(a, b, "{wild:?} must clamp at apply");
        }
    }
}
