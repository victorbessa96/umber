//! Named brush presets and their `.umberbrush` JSON file format.
//!
//! Wave-4 slices 1–2 (docs/specs/brush-presets-design.md): the data model.
//! A [`BrushPreset`] binds every parameter the engine already exposes —
//! [`BrushParams`](crate::BrushParams) (dab shape + response),
//! [`OneEuroParams`](crate::OneEuroParams) (stabilizer),
//! [`LazyMouseConfig`] + [`SpacingConfig`] (plain-data extracts of the
//! [`LazyMouse`](crate::LazyMouse) and
//! [`SpacingAccumulator`](crate::SpacingAccumulator) tunings) — plus two
//! 4-point pressure→dab [`ControlCurve`] mappings (pressure→alpha,
//! pressure→radius).
//!
//! The file envelope is `{ "format": "umber-brush-preset", "version": 1,
//! ...preset fields }`, written with `serde_json::to_string_pretty` so
//! write→read→write is byte-identical. Forward compatibility: every
//! [`BrushPreset`] field carries `#[serde(default)]`, so JSON missing a key
//! loads with that field's default instead of failing.
//!
//! Wave 3.1b adds one optional field: [`PatternTexture`], the brush texture
//! an `.abr` import carries beside the preset as a `<slug>.pattern.png`
//! sidecar. It is `None` for every hand-authored preset, and a `None` field
//! is not written at all, so pre-texture files round-trip byte-identically.
//! Nothing samples the texture yet: kill 3.1c is the paint-engine consumer.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::{BrushParams, OneEuroParams};

/// File-format discriminator written into every `.umberbrush` file.
pub const PRESET_FORMAT: &str = "umber-brush-preset";
/// The only `.umberbrush` version this code reads and writes.
pub const PRESET_VERSION: u32 = 1;

fn default_format() -> String {
    PRESET_FORMAT.to_owned()
}

fn default_version() -> u32 {
    PRESET_VERSION
}

/// Strict-construction and file I/O errors for presets.
///
/// Follows the crate's `thiserror` style (cf. `OneEuroError`,
/// `SpacingError`). The `serde_json::Error` is never exposed with `#[from]`:
/// [`BrushPreset::from_json`] wraps it manually so the carrying variant
/// records the source line/column where parsing stopped.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum PresetError {
    /// Malformed JSON: 1-based source line/column plus serde's message.
    #[error("invalid preset JSON at line {source_line}, column {source_column}: {message}")]
    Json {
        source_line: usize,
        source_column: usize,
        message: String,
    },
    /// The `format` envelope field is not [`PRESET_FORMAT`].
    #[error("wrong preset format: expected \"umber-brush-preset\", found \"{found}\"")]
    WrongFormat { found: String },
    /// The `version` envelope field is not [`PRESET_VERSION`].
    #[error("unsupported preset version: expected 1, found {found}")]
    UnsupportedVersion { found: u32 },
    /// Control-curve x coordinates are not strictly increasing
    /// (equal x counts: the bracketing segment would divide by zero).
    #[error("control-curve x values must be strictly increasing")]
    NonMonotonicX,
    /// A control-curve coordinate or parameter is NaN/infinite.
    #[error("control-curve points must be finite")]
    NonFinite,
    /// The preset name is empty.
    #[error("preset name must not be empty")]
    EmptyName,
    /// A parameter is outside its engine domain: `alpha`/`hardness` outside
    /// 0..=1, `pressure_gamma` non-finite or <= 0, one-euro fields outside
    /// their `try_new` domain, spacing density outside its `new` domain, a
    /// control curve built with the wrong point count, or a texture block
    /// whose `file` is not a bare file name / whose `scale` or `depth` is
    /// outside `0 < x <= 100`.
    #[error("preset parameter outside its valid domain")]
    InvalidParams,
}

/// A 4-point input→dab control curve (pressure→alpha, pressure→radius).
///
/// Exactly four `(x, y)` points are stored in [`Self::points`] (array, not
/// `Vec`, so the count is fixed by the type once constructed). Invariants,
/// enforced by [`Self::try_new`] and by `Deserialize`:
/// - every coordinate is finite ([`PresetError::NonFinite`] otherwise);
/// - x is strictly increasing, `x[0] < x[1] < x[2] < x[3]`
///   ([`PresetError::NonMonotonicX`] otherwise — equal x is a rejection,
///   since the bracketing segment would divide by zero).
///
/// JSON shape is the bare array `[[x,y] x 4]` (custom `Serialize`/
/// `Deserialize` impls — a derived struct would nest under a `"points"`
/// key, which the `.umberbrush` envelope does not use).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlCurve {
    /// The four control points, strictly increasing in x.
    pub points: [(f32, f32); 4],
}

impl ControlCurve {
    /// Build a curve, enforcing the finite + strictly-increasing-x
    /// invariants. Takes a slice (rather than the fixed array) so a wrong
    /// point count is expressible: anything but 4 points is rejected with
    /// [`PresetError::InvalidParams`].
    ///
    /// # Errors
    ///
    /// Returns [`PresetError::InvalidParams`] for a length other than 4,
    /// [`PresetError::NonFinite`] for NaN/infinite coordinates, and
    /// [`PresetError::NonMonotonicX`] when x is not strictly increasing.
    pub fn try_new(points: &[(f32, f32)]) -> Result<Self, PresetError> {
        if points.len() != 4 {
            return Err(PresetError::InvalidParams);
        }
        if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
            return Err(PresetError::NonFinite);
        }
        if !(points[0].0 < points[1].0 && points[1].0 < points[2].0 && points[2].0 < points[3].0) {
            return Err(PresetError::NonMonotonicX);
        }
        Ok(Self {
            points: [points[0], points[1], points[2], points[3]],
        })
    }

    /// The identity mapping: `[(0,0), (1/3,1/3), (2/3,2/3), (1,1)]` exactly
    /// (f32 `1.0/3.0` etc.). `pressure_gamma` on [`BrushParams`] remains the
    /// shortcut for this case.
    pub fn identity() -> Self {
        Self {
            points: [
                (0.0, 0.0),
                (1.0 / 3.0, 1.0 / 3.0),
                (2.0 / 3.0, 2.0 / 3.0),
                (1.0, 1.0),
            ],
        }
    }

    /// Clamped piecewise-linear interpolation: below `x[0]` yields `y[0]`,
    /// above `x[3]` yields `y[3]`, otherwise the lerp between the two points
    /// bracketing `x`.
    pub fn evaluate(&self, x: f32) -> f32 {
        let p = self.points;
        if x <= p[0].0 {
            return p[0].1;
        }
        if x >= p[3].0 {
            return p[3].1;
        }
        for i in 0..3 {
            if x <= p[i + 1].0 {
                let (x0, y0) = p[i];
                let (x1, y1) = p[i + 1];
                let t = (x - x0) / (x1 - x0);
                return y0 + (y1 - y0) * t;
            }
        }
        p[3].1
    }
}

impl Default for ControlCurve {
    fn default() -> Self {
        Self::identity()
    }
}

impl Serialize for ControlCurve {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.points.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ControlCurve {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let points = Vec::<(f32, f32)>::deserialize(deserializer)?;
        if points.len() != 4 {
            return Err(serde::de::Error::invalid_length(
                points.len(),
                &"exactly 4 control points",
            ));
        }
        Self::try_new(&points).map_err(serde::de::Error::custom)
    }
}

/// Plain-data tuning for the pulled-string smoother.
///
/// NOTE: the engine's [`LazyMouse`](crate::LazyMouse) currently takes only
/// `radius_px` at construction (`LazyMouse::new(radius_px)`) and holds no
/// strength state. This config struct is plain data; wiring a preset into
/// the engine happens at the call site (`LazyMouse::new(cfg.radius_px)`),
/// where `strength` is carried for the future pull-strength tuning and
/// ignored today. That wiring is a later slice, not this module.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LazyMouseConfig {
    /// Leash length in logical pixels.
    pub radius_px: f32,
    /// Pull strength (reserved for the call-site wiring; see above).
    pub strength: f32,
}

impl Default for LazyMouseConfig {
    fn default() -> Self {
        Self {
            radius_px: 12.0,
            strength: 0.75,
        }
    }
}

/// Plain-data tuning for the dab-spacing engine.
///
/// Domain mirrors [`SpacingAccumulator::new`](crate::SpacingAccumulator::new):
/// the density must be finite and strictly positive. Default is 12.0 dabs
/// per radius.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpacingConfig {
    /// Dabs emitted per one brush-radius of travel.
    pub dabs_per_radius: f32,
}

impl Default for SpacingConfig {
    fn default() -> Self {
        Self {
            dabs_per_radius: 12.0,
        }
    }
}

/// A brush's tiling texture: the `.abr` import's `patt` pattern, persisted
/// as a `<slug>.pattern.png` sidecar beside the preset and referenced here
/// by bare file name.
///
/// Wave 3.1b. Every field carries a serde default, so a partial or
/// forward-written block still loads; `scale`/`depth` default to `1.0`
/// (100%) rather than `0.0`, because a zero scale is outside the domain
/// [`BrushPreset::validate`] enforces and a defaulted block should load
/// valid, not rejected.
///
/// `scale`/`depth` are fractions (`1.0` = 100%), the shape
/// [`AbrImportTexture`](crate::abr_import::AbrImportTexture) already
/// speaks. [`BrushPreset::validate`] bounds them by `0 < x <= 100` — wider
/// than a fraction needs — so a percent-valued file from another authoring
/// path also loads; the engine consumer (kill 3.1c) decides which it
/// samples.
///
/// `blend_mode` is the stored `.abr` label, undecoded (e.g.
/// `"BlnM.CBrn"`): naming the mode is 3.1c's call, so the import preserves
/// what the file said rather than guessing a umber mode. Empty means "the
/// file did not say".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternTexture {
    /// Sidecar file name relative to the preset, e.g.
    /// `"chalk.pattern.png"`. A bare file name: [`BrushPreset::validate`]
    /// rejects path separators, so a preset cannot point outside its own
    /// directory.
    #[serde(default)]
    pub file: String,
    /// Texture scale as a fraction (`1.0` = 100%); finite and `> 0`.
    #[serde(default = "default_texture_level")]
    pub scale: f32,
    /// Texture depth as a fraction (`1.0` = 100%); finite and `> 0`.
    #[serde(default = "default_texture_level")]
    pub depth: f32,
    /// Whether the pattern is inverted.
    #[serde(default)]
    pub invert: bool,
    /// The stored blend-mode label, undecoded (see the type docs).
    #[serde(default)]
    pub blend_mode: String,
    /// Whether the texture applies to each tip rather than per stroke.
    #[serde(default)]
    pub each_tip: bool,
}

fn default_texture_level() -> f32 {
    1.0
}

impl Default for PatternTexture {
    fn default() -> Self {
        // Tuned default (a level of 1.0, not the derived 0.0): a
        // default-constructed block must pass `validate()`.
        Self {
            file: String::new(),
            scale: default_texture_level(),
            depth: default_texture_level(),
            invert: false,
            blend_mode: String::new(),
            each_tip: false,
        }
    }
}

/// A named, saveable brush: the full engine parameter set plus curves.
///
/// Every field carries `#[serde(default)]` (the forward-compat rule): JSON
/// missing a key loads with that field's default instead of failing, so
/// older files and hand-written partial files keep loading as new fields
/// are added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrushPreset {
    /// Display name (must be non-empty per [`Self::validate`]).
    #[serde(default)]
    pub name: String,
    /// Dab color/shape/response.
    #[serde(default)]
    pub params: BrushParams,
    /// Stroke stabilizer tuning.
    #[serde(default)]
    pub one_euro: OneEuroParams,
    /// Pulled-string smoother tuning (plain data; see [`LazyMouseConfig`]).
    #[serde(default)]
    pub lazy_mouse: LazyMouseConfig,
    /// Dab-density tuning.
    #[serde(default)]
    pub spacing: SpacingConfig,
    /// Pressure→alpha mapping.
    #[serde(default)]
    pub alpha_curve: ControlCurve,
    /// Pressure→radius mapping.
    #[serde(default)]
    pub radius_curve: ControlCurve,
    /// How pointer motion becomes dabs (Space / Airbrush / Dash / Drag
    /// Dot; [`crate::StrokeMethod`]). `#[serde(default)]` = Space, so every
    /// pre-method `.umberbrush` loads with its historical spacing behavior.
    #[serde(default)]
    pub stroke_method: crate::StrokeMethod,
    /// The brush size multiplier on the paint session's base radius
    /// (`1.0` = the historical 24-texel brush; `[` / `]` and Ctrl+RMB
    /// drag write it). `#[serde(default = ...)]` keeps older files at 1.
    #[serde(default = "default_size")]
    pub size: f32,
    /// The asset catalog this preset files under in the brush shelf and
    /// popup (UI/UX pass U3, Blender 4.3's catalogs): a free-form label
    /// such as `"Basic"` or `"Erase"`. Empty = uncategorised; an empty
    /// value is not written, so pre-catalog files round-trip unchanged.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub catalog: String,
    /// The brush's tiling texture (wave 3.1b; [`PatternTexture`]): the
    /// `.abr` import's `patt` pattern, kept as a sidecar beside this
    /// preset. `None` = no texture, and the key is not written at all, so
    /// pre-texture files round-trip byte-identically. Declared last so the
    /// serialized key order of every older field is unchanged. Nothing
    /// samples it yet (kill 3.1c is the engine consumer); the block is
    /// persisted now because the import is one-shot and re-running it
    /// would hit the import's no-overwrite refusal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<PatternTexture>,
}

fn default_size() -> f32 {
    1.0
}

impl BrushPreset {
    /// The engine params this preset paints with: `params` plus the
    /// preset's per-property pressure curves (U3-5): `alpha_curve` →
    /// flow, `radius_curve` → size. Before this the two curves were
    /// edited and saved but never reached the dab engine.
    pub fn engine_params(&self) -> crate::BrushParams {
        crate::BrushParams {
            pressure_maps: crate::dab_adapter::PressureMaps {
                flow: self.alpha_curve,
                size: self.radius_curve,
            },
            ..self.params
        }
    }

    /// Serialize to the `.umberbrush` envelope
    /// (`format`/`version` header + preset fields, pretty-printed; key order
    /// follows struct field order, so equal structs produce byte-identical
    /// output).
    ///
    /// # Errors
    ///
    /// Returns [`PresetError::Json`] if serialization fails (line/column
    /// are 0: serialization errors carry no source position).
    pub fn to_json(&self) -> Result<String, PresetError> {
        let file = PresetFile {
            format: PRESET_FORMAT.to_owned(),
            version: PRESET_VERSION,
            preset: self.clone(),
        };
        serde_json::to_string_pretty(&file).map_err(|e| PresetError::Json {
            source_line: e.line(),
            source_column: e.column(),
            message: e.to_string(),
        })
    }

    /// Parse a `.umberbrush` document, verifying the envelope before
    /// returning the preset.
    ///
    /// # Errors
    ///
    /// Returns [`PresetError::Json`] for malformed JSON (with the source
    /// line/column where parsing stopped), [`PresetError::WrongFormat`]
    /// when `format` is not [`PRESET_FORMAT`], and
    /// [`PresetError::UnsupportedVersion`] when `version` is not
    /// [`PRESET_VERSION`].
    pub fn from_json(s: &str) -> Result<Self, PresetError> {
        let file: PresetFile = serde_json::from_str(s).map_err(|e| PresetError::Json {
            source_line: e.line(),
            source_column: e.column(),
            message: e.to_string(),
        })?;
        if file.format != PRESET_FORMAT {
            return Err(PresetError::WrongFormat { found: file.format });
        }
        if file.version != PRESET_VERSION {
            return Err(PresetError::UnsupportedVersion {
                found: file.version,
            });
        }
        Ok(file.preset)
    }

    /// Check the preset against the engine's real parameter domains.
    ///
    /// One-euro constraints mirror
    /// [`OneEuroFilter::try_new`](crate::OneEuroFilter::try_new)
    /// (`one_euro.rs` `validate`): `min_cutoff` finite and > 0, `beta`
    /// finite and >= 0, `d_cutoff` finite and > 0. Spacing mirrors
    /// [`SpacingAccumulator::new`](crate::SpacingAccumulator::new):
    /// `dabs_per_radius` finite and > 0. Curves hold by construction
    /// ([`ControlCurve::try_new`]) and are re-checked defensively.
    ///
    /// # Errors
    ///
    /// Returns [`PresetError::EmptyName`] for an empty name,
    /// [`PresetError::InvalidParams`] for any out-of-domain parameter, and
    /// [`PresetError::NonMonotonicX`]/[`PresetError::NonFinite`] if a curve
    /// somehow violates its invariants.
    ///
    /// A present [`PatternTexture`] is checked too: its `file` must be a
    /// bare file name (non-empty, not `.`/`..`, no `/`, `\`, `:` or NUL)
    /// and `scale`/`depth` must be finite in `0 < x <= 100`. A preset
    /// without a texture skips both rules.
    pub fn validate(&self) -> Result<(), PresetError> {
        if self.name.is_empty() {
            return Err(PresetError::EmptyName);
        }
        if !(0.0..=1.0).contains(&self.params.alpha)
            || !(0.0..=1.0).contains(&self.params.hardness)
            || !self.params.pressure_gamma.is_finite()
            || self.params.pressure_gamma <= 0.0
        {
            return Err(PresetError::InvalidParams);
        }
        if !self.one_euro.min_cutoff.is_finite()
            || self.one_euro.min_cutoff <= 0.0
            || !self.one_euro.beta.is_finite()
            || self.one_euro.beta < 0.0
            || !self.one_euro.d_cutoff.is_finite()
            || self.one_euro.d_cutoff <= 0.0
        {
            return Err(PresetError::InvalidParams);
        }
        if !self.spacing.dabs_per_radius.is_finite() || self.spacing.dabs_per_radius <= 0.0 {
            return Err(PresetError::InvalidParams);
        }
        for curve in [
            &self.alpha_curve,
            &self.radius_curve,
            &self.params.falloff_custom,
        ] {
            let p = curve.points;
            if p.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
                return Err(PresetError::NonFinite);
            }
            if !(p[0].0 < p[1].0 && p[1].0 < p[2].0 && p[2].0 < p[3].0) {
                return Err(PresetError::NonMonotonicX);
            }
        }
        // The size multiplier is the path tool's domain (0.05..=8). The
        // stroke method validates itself on deserialize; this keeps a
        // struct built in code to the same rule.
        if !self.size.is_finite() || !(0.05..=8.0).contains(&self.size) {
            return Err(PresetError::InvalidParams);
        }
        if self.stroke_method.validate().is_err() {
            return Err(PresetError::InvalidParams);
        }
        // The texture block (wave 3.1b). `file` names a sidecar beside the
        // preset, so it must be a bare file name: a separator would let a
        // preset point into another directory, and `.`/`..` name a
        // directory rather than a file. `:` is refused too — it is a path
        // separator on some platforms and cannot appear in a portable
        // sidecar name.
        if let Some(texture) = &self.texture {
            let file = texture.file.as_str();
            let has_path_syntax = file.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0'));
            if file.is_empty() || file == "." || file == ".." || has_path_syntax {
                return Err(PresetError::InvalidParams);
            }
            // Fractions (1.0 = 100%) bounded by the wide 0 < x <= 100
            // domain: a fraction is well inside it, and a percent-valued
            // file from another authoring path still loads. NaN and the
            // infinities fail `is_finite` before the range compares.
            for level in [texture.scale, texture.depth] {
                if !level.is_finite() || level <= 0.0 || level > 100.0 {
                    return Err(PresetError::InvalidParams);
                }
            }
        }
        Ok(())
    }
}

/// The on-disk `.umberbrush` envelope: `format`/`version` header plus the
/// preset fields flattened alongside (so the file has a single flat
/// namespace: `format`, `version`, `name`, `params`, ...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetFile {
    /// Must be [`PRESET_FORMAT`]; defaults to it when the key is missing.
    #[serde(default = "default_format")]
    pub format: String,
    /// Must be [`PRESET_VERSION`]; defaults to it when the key is missing.
    #[serde(default = "default_version")]
    pub version: u32,
    /// The preset body.
    #[serde(flatten)]
    pub preset: BrushPreset,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FalloffCurve, WashMode};

    const EPS: f32 = 1e-6;

    fn non_default_preset() -> BrushPreset {
        BrushPreset {
            name: "Test Chisel".to_owned(),
            params: BrushParams {
                color: [0.1, 0.2, 0.3, 0.9],
                alpha: 0.33,
                hardness: 0.77,
                pressure_gamma: 2.2,
                pressure_curve: ControlCurve::identity(),
                wash_mode: WashMode::Wash,
                falloff: FalloffCurve::Sphere,
                falloff_custom: ControlCurve::try_new(&[
                    (0.0, 0.0),
                    (0.4, 0.7),
                    (0.8, 0.9),
                    (1.0, 1.0),
                ])
                .expect("valid test curve"),
                stamped: false,
                stamp_angle: Default::default(),
                jitter: Default::default(),
                pressure_maps: Default::default(),
            },
            one_euro: OneEuroParams {
                min_cutoff: 2.5,
                beta: 0.05,
                d_cutoff: 0.8,
            },
            lazy_mouse: LazyMouseConfig {
                radius_px: 20.0,
                strength: 0.5,
            },
            spacing: SpacingConfig {
                dabs_per_radius: 8.0,
            },
            alpha_curve: ControlCurve::try_new(&[(0.0, 0.0), (0.25, 0.1), (0.75, 0.9), (1.0, 1.0)])
                .expect("valid test curve"),
            radius_curve: ControlCurve::try_new(&[(0.0, 0.5), (0.3, 0.6), (0.7, 0.9), (1.0, 1.0)])
                .expect("valid test curve"),
            catalog: "Texture".to_owned(),
            stroke_method: crate::StrokeMethod::Dash {
                ratio: 0.5,
                length_px: 20.0,
            },
            size: 1.5,
            texture: None,
        }
    }

    #[test]
    fn a_preset_without_method_or_size_loads_as_space_at_unit_size() {
        // A pre-2026-10-15 file has neither key: it must load unchanged.
        let mut json: serde_json::Value =
            serde_json::from_str(&non_default_preset().to_json().expect("ser")).expect("json");
        let obj = json.as_object_mut().expect("object");
        assert!(
            obj.remove("stroke_method").is_some(),
            "the fixture writes the key"
        );
        assert!(obj.remove("size").is_some(), "the fixture writes the key");
        let back = BrushPreset::from_json(&json.to_string()).expect("loads");
        assert_eq!(back.stroke_method, crate::StrokeMethod::Space);
        assert_eq!(back.size, 1.0);
    }

    #[test]
    fn round_trip_is_byte_identical() {
        let preset = non_default_preset();
        let first = preset.to_json().expect("serialize");
        let back = BrushPreset::from_json(&first).expect("parse");
        assert_eq!(back, preset, "round trip must preserve every field");
        let second = back.to_json().expect("re-serialize");
        assert_eq!(
            second, first,
            "serde_json pretty output is deterministic: same struct, same bytes"
        );
    }

    #[test]
    fn identity_curve_evaluates_exactly() {
        let curve = ControlCurve::identity();
        assert_eq!(
            curve.points,
            [
                (0.0, 0.0),
                (1.0 / 3.0, 1.0 / 3.0),
                (2.0 / 3.0, 2.0 / 3.0),
                (1.0, 1.0)
            ]
        );
        assert!((curve.evaluate(0.0) - 0.0).abs() <= EPS);
        // Midpoint: x=0.5 lands in segment [1/3, 2/3] at t=0.5, so
        // y = 1/3 + (2/3-1/3)*0.5 = 0.5 (within one f32 rounding step).
        assert!((curve.evaluate(0.5) - 0.5).abs() <= EPS);
        assert!((curve.evaluate(1.0) - 1.0).abs() <= EPS);
        assert!((curve.evaluate(-0.5) - 0.0).abs() <= EPS);
        assert!((curve.evaluate(1.5) - 1.0).abs() <= EPS);
    }

    #[test]
    fn steep_curve_lerps_within_bracketing_segment() {
        let curve = ControlCurve::try_new(&[(0.0, 0.0), (0.33, 0.1), (0.66, 0.9), (1.0, 1.0)])
            .expect("valid test curve");
        // Derivation: x=0.165 falls in segment 0 (0 <= 0.165 < 0.33), so
        // t = (0.165-0)/(0.33-0) = 0.5 and y = 0 + (0.1-0)*0.5 = 0.05.
        let got = curve.evaluate(0.165);
        assert!(
            (got - 0.05).abs() <= 1e-6,
            "bracketing-segment lerp must give ~0.05, got {got}"
        );
    }

    #[test]
    fn try_new_rejects_bad_curves() {
        // Equal x is a rejection (the segment would divide by zero).
        assert_eq!(
            ControlCurve::try_new(&[(0.0, 0.0), (0.0, 1.0), (1.0, 0.0), (1.0, 1.0)]),
            Err(PresetError::NonMonotonicX)
        );
        // Decreasing x is a rejection.
        assert_eq!(
            ControlCurve::try_new(&[(0.0, 0.0), (0.7, 0.5), (0.6, 0.8), (1.0, 1.0)]),
            Err(PresetError::NonMonotonicX)
        );
        // NaN y is a rejection.
        assert_eq!(
            ControlCurve::try_new(&[(0.0, 0.0), (0.3, f32::NAN), (0.6, 0.8), (1.0, 1.0)]),
            Err(PresetError::NonFinite)
        );
        // Infinite x is a rejection.
        assert_eq!(
            ControlCurve::try_new(&[(0.0, 0.0), (0.3, 0.5), (f32::INFINITY, 0.8), (1.0, 1.0)]),
            Err(PresetError::NonFinite)
        );
        // Three points is a rejection.
        assert!(ControlCurve::try_new(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]).is_err());
    }

    #[test]
    fn from_json_gates_format_version_and_syntax() {
        let valid = non_default_preset().to_json().expect("serialize");

        let wrong_format = valid.replace(PRESET_FORMAT, "umber-brush-preset-v2");
        assert_eq!(
            BrushPreset::from_json(&wrong_format),
            Err(PresetError::WrongFormat {
                found: "umber-brush-preset-v2".to_owned()
            })
        );

        let wrong_version = valid.replace("\"version\": 1", "\"version\": 2");
        assert_eq!(
            BrushPreset::from_json(&wrong_version),
            Err(PresetError::UnsupportedVersion { found: 2 })
        );

        let truncated = &valid[..valid.len() / 2];
        match BrushPreset::from_json(truncated) {
            Err(PresetError::Json {
                source_line,
                source_column,
                message,
            }) => {
                assert!(source_line >= 1, "line info must be present");
                assert!(source_column >= 1, "column info must be present");
                assert!(!message.is_empty());
            }
            other => panic!("truncated JSON must fail with Json, got {other:?}"),
        }
    }

    #[test]
    fn from_json_missing_key_falls_back_to_default() {
        let valid = non_default_preset().to_json().expect("serialize");
        let mut value: serde_json::Value = serde_json::from_str(&valid).expect("valid JSON");
        value
            .as_object_mut()
            .expect("top-level object")
            .remove("spacing");
        let without_spacing = serde_json::to_string_pretty(&value).expect("re-serialize");
        let back = BrushPreset::from_json(&without_spacing).expect("missing key must default");
        assert_eq!(back.spacing, SpacingConfig::default());
        assert_eq!(back.name, "Test Chisel");
    }

    #[test]
    fn wash_mode_round_trips_and_old_files_default_to_build_up() {
        // The fixture preset carries Wash: the byte-identical round trip
        // above already proves the mode survives a save/load cycle.
        assert_eq!(non_default_preset().params.wash_mode, WashMode::Wash);

        // A pre-mode file (no `wash_mode` key under `params`) loads with
        // historical build-up behavior instead of failing.
        let valid = non_default_preset().to_json().expect("serialize");
        let mut value: serde_json::Value = serde_json::from_str(&valid).expect("valid JSON");
        value
            .as_object_mut()
            .expect("top-level object")
            .get_mut("params")
            .expect("params object")
            .as_object_mut()
            .expect("params is an object")
            .remove("wash_mode");
        let legacy = serde_json::to_string_pretty(&value).expect("re-serialize");
        let back = BrushPreset::from_json(&legacy).expect("missing wash_mode must default");
        assert_eq!(back.params.wash_mode, WashMode::BuildUp);
        back.validate().expect("legacy preset must validate");
        // ... while the Wash original validates too.
        non_default_preset()
            .validate()
            .expect("wash preset must validate");
    }

    #[test]
    fn falloff_round_trips_and_old_files_default_to_smooth() {
        // The fixture carries Sphere: the byte-identical round trip proves
        // the curve survives save/load; the name is lowercase on disk.
        let valid = non_default_preset().to_json().expect("serialize");
        assert!(valid.contains(r#""falloff": "sphere""#), "{valid}");
        assert_eq!(
            BrushPreset::from_json(&valid)
                .expect("round trip")
                .params
                .falloff,
            FalloffCurve::Sphere
        );

        // A pre-curve file (no `falloff` key under `params`) loads with the
        // historical smoothstep instead of failing, and validates.
        let mut value: serde_json::Value = serde_json::from_str(&valid).expect("valid JSON");
        value
            .as_object_mut()
            .expect("top-level object")
            .get_mut("params")
            .expect("params object")
            .as_object_mut()
            .expect("params is an object")
            .remove("falloff");
        let legacy = serde_json::to_string_pretty(&value).expect("re-serialize");
        let back = BrushPreset::from_json(&legacy).expect("missing falloff must default");
        assert_eq!(back.params.falloff, FalloffCurve::Smooth);
        back.validate().expect("legacy preset must validate");
    }

    /// A `.umberbrush` on the Custom falloff: the curve survives the file
    /// round trip (the fixture carries a drawn curve under Sphere, where
    /// it is inert but still saved), and a Custom file without the
    /// `falloff_custom` key loads with the identity curve and validates.
    #[test]
    fn custom_falloff_files_round_trip_and_default_the_curve() {
        let mut custom = non_default_preset();
        custom.params.falloff = FalloffCurve::Custom;
        let text = custom.to_json().expect("serialize");
        assert!(text.contains(r#""falloff": "custom""#), "{text}");
        assert_eq!(BrushPreset::from_json(&text).expect("round trip"), custom);

        let mut value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        let removed = value
            .as_object_mut()
            .expect("top-level object")
            .get_mut("params")
            .expect("params object")
            .as_object_mut()
            .expect("params is an object")
            .remove("falloff_custom");
        assert!(removed.is_some(), "the fixture writes the key");
        let legacy = serde_json::to_string_pretty(&value).expect("re-serialize");
        let back = BrushPreset::from_json(&legacy).expect("missing curve must default");
        assert_eq!(back.params.falloff, FalloffCurve::Custom);
        assert_eq!(back.params.falloff_custom, ControlCurve::identity());
        back.validate().expect("defaulted custom preset validates");

        // validate() re-checks the falloff curve like the pressure pair.
        let mut bad = non_default_preset();
        bad.params.falloff_custom.points[2].0 = 0.1;
        assert_eq!(bad.validate(), Err(PresetError::NonMonotonicX));
    }

    #[test]
    fn validate_enforces_engine_domains() {
        assert!(non_default_preset().validate().is_ok());

        let mut empty_name = non_default_preset();
        empty_name.name.clear();
        assert_eq!(empty_name.validate(), Err(PresetError::EmptyName));

        let mut bad_alpha = non_default_preset();
        bad_alpha.params.alpha = 1.5;
        assert_eq!(bad_alpha.validate(), Err(PresetError::InvalidParams));

        let mut bad_gamma = non_default_preset();
        bad_gamma.params.pressure_gamma = 0.0;
        assert_eq!(bad_gamma.validate(), Err(PresetError::InvalidParams));

        let mut bad_euro = non_default_preset();
        bad_euro.one_euro.beta = f32::NAN;
        assert_eq!(bad_euro.validate(), Err(PresetError::InvalidParams));

        let mut bad_spacing = non_default_preset();
        bad_spacing.spacing.dabs_per_radius = -3.0;
        assert_eq!(bad_spacing.validate(), Err(PresetError::InvalidParams));
        for size in [0.0, 9.0, f32::NAN] {
            let mut bad = non_default_preset();
            bad.size = size;
            assert_eq!(
                bad.validate(),
                Err(PresetError::InvalidParams),
                "size {size}"
            );
        }
        let mut bad = non_default_preset();
        bad.stroke_method = crate::StrokeMethod::Airbrush { rate_hz: -1.0 };
        assert_eq!(bad.validate(), Err(PresetError::InvalidParams));
    }

    #[test]
    fn shipped_library_loads_and_validates() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/brushes");
        let mut count = 0;
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .expect("assets/brushes must exist")
            .map(|entry| entry.expect("readable dir entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "umberbrush"))
            .collect();
        names.sort();
        for path in names {
            let text = std::fs::read_to_string(&path).expect("readable preset file");
            let preset = BrushPreset::from_json(&text)
                .unwrap_or_else(|e| panic!("{} must parse: {e}", path.display()));
            preset
                .validate()
                .unwrap_or_else(|e| panic!("{} must validate: {e}", path.display()));
            count += 1;
        }
        assert_eq!(count, 18, "the starter library ships eighteen presets");
    }

    /// U3-5: the preset's own curves reach the engine params, and the
    /// shipped "Pressure Round" (the one shipped size curve) now thins
    /// at light pressure.
    #[test]
    fn engine_params_carry_the_preset_pressure_curves() {
        let p = non_default_preset();
        let e = p.engine_params();
        assert_eq!(e.pressure_maps.flow, p.alpha_curve);
        assert_eq!(e.pressure_maps.size, p.radius_curve);
        assert_eq!(
            crate::BrushParams {
                pressure_maps: Default::default(),
                ..e
            },
            p.params
        );
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/brushes/pressure-round.umberbrush"
        );
        let round = BrushPreset::from_json(&std::fs::read_to_string(path).unwrap()).unwrap();
        let scale = round.engine_params().pressure_maps.size_scale(0.25);
        assert!(scale < 0.6, "light pressure thins Pressure Round: {scale}");
    }

    /// U3 catalogs: an old file without `catalog` loads uncategorised and
    /// re-serializes without the key; every shipped preset is filed.
    #[test]
    fn catalogs_default_empty_and_every_shipped_preset_is_filed() {
        let mut old = non_default_preset();
        old.catalog.clear();
        let json = old.to_json().expect("serialize");
        assert!(
            !json.contains("\"catalog\""),
            "empty catalog is not written"
        );
        let back = BrushPreset::from_json(&json).expect("parse");
        assert_eq!(back.catalog, "");
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/brushes");
        for entry in std::fs::read_dir(dir).expect("assets/brushes") {
            let path = entry.expect("entry").path();
            if path.extension().is_some_and(|e| e == "umberbrush") {
                let p = BrushPreset::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
                assert!(!p.catalog.is_empty(), "{} has a catalog", path.display());
            }
        }
    }

    /// The 2026-11-06 brush-starter wave (assets lane): four original
    /// stamped presets, each carrying its `<stem>.stamp.png` sidecar.
    const STARTER_WAVE_PRESETS: [&str; 4] = [
        "flat-liner.umberbrush",
        "soft-pencil.umberbrush",
        "speckle.umberbrush",
        "wide-marker.umberbrush",
    ];

    #[test]
    fn starter_wave_presets_round_trip_byte_identical() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/brushes");
        for file in STARTER_WAVE_PRESETS {
            let path = std::path::Path::new(dir).join(file);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} must exist: {e}", path.display()));
            let preset = BrushPreset::from_json(&text)
                .unwrap_or_else(|e| panic!("{} must parse: {e}", path.display()));
            preset
                .validate()
                .unwrap_or_else(|e| panic!("{} must validate: {e}", path.display()));
            assert!(preset.params.stamped, "{file} is a stamped starter preset");
            let again = preset.to_json().expect("re-serialize");
            assert_eq!(again, text, "{file} must re-serialize byte-identically");
        }
    }

    /// Std-only structural PNG parse (magic, per-chunk CRCs, IHDR fields,
    /// IEND, no trailing bytes): the crate has no image dependency, so the
    /// sidecar pin checks the envelope here; assets/gen/check_stamps.py
    /// re-checks it outside the build with a full IDAT inflate.
    fn png_rgba8_envelope(bytes: &[u8]) -> Result<(u32, u32), String> {
        const MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for &byte in data {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    let mask = (crc & 1).wrapping_neg();
                    crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
                }
            }
            !crc
        }
        if bytes.len() < 8 + 12 || bytes[..8] != MAGIC {
            return Err("bad PNG magic".to_owned());
        }
        let mut pos = 8usize;
        let mut dims = None;
        while pos + 8 <= bytes.len() {
            let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            let ctype = &bytes[pos + 4..pos + 8];
            if pos + 12 + len > bytes.len() {
                return Err(format!(
                    "truncated {} chunk",
                    String::from_utf8_lossy(ctype)
                ));
            }
            let crc = u32::from_be_bytes(bytes[pos + 8 + len..pos + 12 + len].try_into().unwrap());
            if crc != crc32(&bytes[pos + 4..pos + 8 + len]) {
                return Err(format!(
                    "bad CRC in {} chunk",
                    String::from_utf8_lossy(ctype)
                ));
            }
            match ctype {
                b"IHDR" => {
                    let data = &bytes[pos + 8..pos + 8 + len];
                    if len != 13 {
                        return Err("IHDR length must be 13".to_owned());
                    }
                    if data[8] != 8
                        || data[9] != 6
                        || data[10] != 0
                        || data[11] != 0
                        || data[12] != 0
                    {
                        return Err("sidecar must be 8-bit RGBA, non-interlaced".to_owned());
                    }
                    let w = u32::from_be_bytes(data[0..4].try_into().unwrap());
                    let h = u32::from_be_bytes(data[4..8].try_into().unwrap());
                    dims = Some((w, h));
                }
                b"IEND" => {
                    pos += 12 + len;
                    if pos != bytes.len() {
                        return Err("trailing bytes after IEND".to_owned());
                    }
                    return dims.ok_or_else(|| "no IHDR chunk".to_owned());
                }
                _ => {}
            }
            pos += 12 + len;
        }
        Err("no IEND chunk".to_owned())
    }

    #[test]
    fn starter_wave_stamp_sidecars_exist_and_parse_as_rgba8_png() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/brushes");
        for file in STARTER_WAVE_PRESETS {
            let path = std::path::Path::new(dir).join(file);
            let preset = BrushPreset::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert!(
                preset.params.stamped,
                "{file} keys its stamp load on stamped:true"
            );
            let sidecar = path.with_extension("stamp.png");
            let bytes = std::fs::read(&sidecar)
                .unwrap_or_else(|e| panic!("{} must exist: {e}", sidecar.display()));
            let (w, h) = png_rgba8_envelope(&bytes)
                .unwrap_or_else(|e| panic!("{} must parse as PNG: {e}", sidecar.display()));
            assert!(w > 0 && h > 0, "non-empty stamp");
        }
    }

    /// Starter wave 2 (assets lane): two more original stamped presets,
    /// each with a `<stem>.stamp.png` sidecar from make_brush_stamps.py.
    const STARTER2_WAVE_PRESETS: [&str; 2] = ["chalk.umberbrush", "crosshatch.umberbrush"];

    #[test]
    fn starter2_wave_presets_round_trip_byte_identical() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/brushes");
        for file in STARTER2_WAVE_PRESETS {
            let path = std::path::Path::new(dir).join(file);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} must exist: {e}", path.display()));
            let preset = BrushPreset::from_json(&text)
                .unwrap_or_else(|e| panic!("{} must parse: {e}", path.display()));
            preset
                .validate()
                .unwrap_or_else(|e| panic!("{} must validate: {e}", path.display()));
            assert!(preset.params.stamped, "{file} is a stamped starter preset");
            assert!(!preset.catalog.is_empty(), "{file} is filed in a catalog");
            let again = preset.to_json().expect("re-serialize");
            assert_eq!(again, text, "{file} must re-serialize byte-identically");
        }
    }

    #[test]
    fn starter2_wave_stamp_sidecars_exist_and_parse_as_rgba8_png() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/brushes");
        for file in STARTER2_WAVE_PRESETS {
            let sidecar = std::path::Path::new(dir)
                .join(file)
                .with_extension("stamp.png");
            let bytes = std::fs::read(&sidecar)
                .unwrap_or_else(|e| panic!("{} must exist: {e}", sidecar.display()));
            let (w, h) = png_rgba8_envelope(&bytes)
                .unwrap_or_else(|e| panic!("{} must parse as PNG: {e}", sidecar.display()));
            assert_eq!((w, h), (128, 128), "{file}: generator emits 128x128 stamps");
        }
    }

    /// Wave 3.1b: the texture block is additive. A preset without one
    /// writes no `"texture"` key, and a shipped pre-texture file still
    /// re-serializes byte-identically with `texture: None`.
    #[test]
    fn presets_without_a_texture_write_no_texture_key() {
        let fixture = non_default_preset().to_json().expect("serialize");
        assert!(
            !fixture.contains("\"texture\""),
            "a None texture must not be written: {fixture}"
        );
        assert!(BrushPreset::from_json(&fixture)
            .expect("parse")
            .texture
            .is_none());

        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/brushes/flat-liner.umberbrush"
        );
        let text = std::fs::read_to_string(path).expect("shipped preset");
        let preset = BrushPreset::from_json(&text).expect("pre-texture file parses");
        assert!(preset.texture.is_none());
        assert_eq!(
            preset.to_json().expect("re-serialize"),
            text,
            "a pre-texture preset must round-trip byte-identically"
        );
    }

    /// Wave 3.1b: a textured preset keeps every field through the file
    /// round trip, and a block missing `scale`/`depth` loads at 100%.
    #[test]
    fn textured_preset_round_trips_and_defaults_missing_levels() {
        let mut preset = non_default_preset();
        preset.texture = Some(PatternTexture {
            file: "test-chisel.pattern.png".to_owned(),
            scale: 0.66,
            depth: 1.0,
            invert: true,
            blend_mode: "BlnM.CBrn".to_owned(),
            each_tip: true,
        });
        preset.validate().expect("a textured preset validates");
        let text = preset.to_json().expect("serialize");
        assert!(
            text.contains("\"file\": \"test-chisel.pattern.png\""),
            "{text}"
        );
        let back = BrushPreset::from_json(&text).expect("parse");
        assert_eq!(back, preset, "the block survives the round trip");
        assert_eq!(back.to_json().expect("re-serialize"), text);

        // A hand-written block with only the file name: every level
        // defaults to 100% (not 0.0, which validate would reject).
        let mut value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        value["texture"] = serde_json::json!({ "file": "x.pattern.png" });
        let sparse = serde_json::to_string_pretty(&value).expect("re-serialize");
        let loaded = BrushPreset::from_json(&sparse).expect("a partial block loads");
        let texture = loaded.texture.as_ref().expect("the block is present");
        assert_eq!(texture.scale, 1.0);
        assert_eq!(texture.depth, 1.0);
        assert!(!texture.invert);
        assert_eq!(texture.blend_mode, "");
        assert!(!texture.each_tip);
        loaded.validate().expect("a partial block validates");
    }

    /// Wave 3.1b: `validate` refuses a texture `file` that is not a bare
    /// file name, and levels outside `0 < x <= 100`.
    #[test]
    fn validate_rejects_a_path_y_texture_file_and_out_of_range_levels() {
        let textured = |file: &str, scale: f32, depth: f32| {
            let mut preset = non_default_preset();
            preset.texture = Some(PatternTexture {
                file: file.to_owned(),
                scale,
                depth,
                ..PatternTexture::default()
            });
            preset.validate()
        };
        assert!(textured("chalk.pattern.png", 1.0, 1.0).is_ok());
        assert!(
            textured("chalk.pattern.png", 0.66, 100.0).is_ok(),
            "the level domain is (0, 100]"
        );
        for file in [
            "",
            ".",
            "..",
            "sub/chalk.png",
            "sub\\chalk.png",
            "C:chalk.png",
            "a\0b",
        ] {
            assert_eq!(
                textured(file, 1.0, 1.0),
                Err(PresetError::InvalidParams),
                "file {file:?}"
            );
        }
        for (scale, depth) in [
            (0.0, 1.0),
            (-1.0, 1.0),
            (100.5, 1.0),
            (f32::NAN, 1.0),
            (1.0, f32::INFINITY),
            (1.0, 0.0),
        ] {
            assert_eq!(
                textured("chalk.pattern.png", scale, depth),
                Err(PresetError::InvalidParams),
                "scale {scale}, depth {depth}"
            );
        }
        // A preset with no texture skips both rules entirely.
        assert!(non_default_preset().validate().is_ok());
    }
}
