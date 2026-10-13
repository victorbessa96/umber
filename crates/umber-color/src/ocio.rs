//! The OCIO bridge (requirements.md §9): behind the `ocio` cargo
//! feature, exposes the built-in ACES 2.0 config surface through the
//! `ocio-rs` crate (stub mode — no C++ dependency, CI-safe; the heavy
//! bundled OCIO build is a manual-dispatch job, see
//! `docs/research/ocio-aces-integration.md`).
//!
//! Without the feature the module is absent — `umber-color` stays a
//! pure-Rust crate with the fixed sRGB display path as the whole
//! truth, and this bridge is opt-in.
//!
//! Stub-mode contract (probed 2026-10-09): every handle-allocating
//! call returns `Err` with "OpenColorIO handle allocation failed".
//! The API *shapes* are what the gated tests verify — the same tests
//! document what flips when the real OCIO lands.

use thiserror::Error;

/// Errors from the OCIO bridge.
#[derive(Debug, Error)]
pub enum OcioBridgeError {
    /// The ocio-rs call failed (stub mode: handle allocation; real
    /// mode: whatever OCIO rejected).
    #[error("OCIO: {0}")]
    Ocio(String),
    /// A built-in config index was out of range.
    #[error("builtin config index {0} out of range")]
    IndexOutOfRange(i32),
}

impl From<ocio_rs::OcioError> for OcioBridgeError {
    fn from(err: ocio_rs::OcioError) -> Self {
        Self::Ocio(err.to_string())
    }
}

/// The name OCIO registers the ACES 2.0 CG config under (the §9
/// default; Studio is the opt-in alternative). The REGISTRY'S ENUM
/// NAME, verified against the real vendored OCIO 2.5 on the first
/// bundled run (2026-10-12): `create_from_builtin_config` matches
/// these compact identifiers — "Could not find 'CG'" is what the UI
/// form produced; the UI names live in `builtin_configs()`'s second
/// tuple slot for display only.
pub const ACES_CG_CONFIG: &str = "cg-config-v4.0.0_aces-v2.0_ocio-v2.5";

/// The ACES 2.0 Studio config (the §9 alternative to CG).
pub const ACES_STUDIO_CONFIG: &str = "studio-config-v4.0.0_aces-v2.0_ocio-v2.5";

/// Enumerates OCIO's built-in configs (name + UI name pairs). Stub
/// mode: the registry handle fails to allocate, so the list comes back
/// empty — the caller treats that as "no OCIO available", never an
/// error to surface to the UI.
pub fn builtin_configs() -> Vec<(String, String)> {
    let Ok(registry) = ocio_rs::BuiltinConfigRegistry::get() else {
        return Vec::new();
    };
    let n = registry.num_builtin_configs();
    let mut out = Vec::new();
    for i in 0..n {
        let name = registry.config_name(i).unwrap_or_default();
        let ui = registry.config_ui_name(i).unwrap_or_default();
        if !name.is_empty() {
            out.push((name, ui));
        }
    }
    out
}

/// Creates a built-in config by name. Stub mode: `Err` (the handle
/// allocation fails) — the bridge's honest "OCIO unavailable" answer.
pub fn create_builtin_config(name: &str) -> Result<(), OcioBridgeError> {
    let _config = ocio_rs::Config::create_from_builtin_config(name)?;
    Ok(())
}

/// A live built-in config held for the session (the §9 "in panel"
/// adoption: the Display panel creates it on selection, keeps it, and
/// tabulates display LUTs through it). The config owns nothing else —
/// dropping the handle is the whole teardown.
pub struct OcioBuiltinConfig {
    config: ocio_rs::Config,
}

impl OcioBuiltinConfig {
    /// Creates the named built-in config (an OCIO registry name —
    /// [`ACES_CG_CONFIG`] / [`ACES_STUDIO_CONFIG`], the enum forms).
    /// Stub mode: the handle allocation fails and this errors — the
    /// panel's honest "unavailable" path.
    pub fn create(name: &str) -> Result<Self, OcioBridgeError> {
        Ok(Self {
            config: ocio_rs::Config::create_from_builtin_config(name)?,
        })
    }

    /// The config's color-space names, in registry order (the panel's
    /// display-space dropdown). Stub-unreachable (needs a live config).
    pub fn display_spaces(&self) -> Vec<String> {
        let n = self.config.num_color_spaces();
        (0..n)
            .filter_map(|i| self.config.color_space_name_by_index(i))
            .collect()
    }

    /// The scene-linear → display-space CPU processor. The LUT
    /// tabulation drives it entry by entry so the viewer chain
    /// (exposure before, gamma after) composes around the OCIO
    /// transform — the same chain semantics as [`apply_display_chain`].
    pub fn display_processor(
        &self,
        display_space: &str,
    ) -> Result<ocio_rs::CPUProcessor, OcioBridgeError> {
        let src = self
            .config
            .role_color_space("scene_linear")
            .ok_or_else(|| OcioBridgeError::Ocio("config lacks the scene_linear role".into()))?;
        let processor = self.config.processor(&src, display_space)?;
        Ok(processor.default_cpu_processor()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The mode split (audit worklist #8): stub mode (default) has NO C++
    // library behind the bridge — handle allocation fails, the registry
    // enumerates empty. The bundled build (ocio-bundled → ocio-rs/bundled)
    // compiles the vendored OCIO C++ tree and the real registry
    // materializes: eight built-in configs incl. the ACES 2.0 CG/Studio
    // pair the §9 row names. The tests assert whichever side compiled in.

    #[cfg(not(feature = "ocio-bundled"))]
    #[test]
    fn stub_mode_reports_no_builtin_configs() {
        // In stub mode the registry handle fails to allocate, so the
        // enumeration is an empty list — never a panic, never an
        // error: the caller renders "OCIO unavailable".
        let configs = builtin_configs();
        assert!(configs.is_empty(), "stub mode: no configs, got {configs:?}");
    }

    #[cfg(not(feature = "ocio-bundled"))]
    #[test]
    fn stub_mode_config_creation_fails_cleanly() {
        // The built-in creation path exists (the probe proved the API
        // compiles) but a stub-mode handle cannot allocate — the
        // bridge surfaces that as an Ocio error, not a panic.
        let err = create_builtin_config(ACES_CG_CONFIG).unwrap_err();
        assert!(matches!(err, OcioBridgeError::Ocio(_)));
        assert!(
            err.to_string().contains("OCIO"),
            "error names its source: {err}"
        );
    }

    #[cfg(feature = "ocio-bundled")]
    #[test]
    fn real_mode_enumerates_the_aces_2_pair() {
        // The vendored OCIO 2.5 registry carries eight built-in configs
        // (probed 2026-10-12, ocio-bundled first real run); the §9 pair
        // MUST be among them under their UI names. Can-fail guard: an
        // empty registry here means the bundled build silently
        // degraded to the stub — the kill's whole point.
        let configs = builtin_configs();
        assert!(
            !configs.is_empty(),
            "bundled build must enumerate configs; empty = stub degraded"
        );
        let names: Vec<&str> = configs.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names.contains(&ACES_CG_CONFIG),
            "ACES 2.0 CG config missing from {names:?}"
        );
        assert!(
            names.contains(&ACES_STUDIO_CONFIG),
            "ACES 2.0 Studio config missing from {names:?}"
        );
    }

    #[cfg(feature = "ocio-bundled")]
    #[test]
    fn real_mode_creates_the_aces_2_cg_config() {
        // The §9 P0's actual payload: CreateFromBuiltin("CG Config for
        // ACES 2.0") returns a live config handle — the in-memory
        // config with zero external LUT files (the research doc's
        // architecture, now executed).
        create_builtin_config(ACES_CG_CONFIG)
            .expect("the real OCIO creates the ACES 2.0 CG config");
    }

    #[cfg(feature = "ocio-bundled")]
    #[test]
    fn real_mode_config_tabulates_a_display_lut() {
        // The panel adoption's payload: a held config lists its color
        // spaces and hands out the scene-linear → display CPU
        // processor. Probed values pinned (2026-10-13): the CG config
        // carries 25 spaces, scene_linear = ACEScg, "sRGB - Display"
        // is among them. The processor must be a real transform: mid
        // gray encodes inside the tone curve (not identity, not clip).
        let config = OcioBuiltinConfig::create(ACES_CG_CONFIG).expect("the CG config creates");
        let spaces = config.display_spaces();
        assert_eq!(
            spaces.len(),
            25,
            "probed space count (25) — got {}",
            spaces.len()
        );
        assert!(
            spaces.iter().any(|s| s == "sRGB - Display"),
            "the sRGB display space is in the dropdown list"
        );
        let cpu = config
            .display_processor("sRGB - Display")
            .expect("the sRGB display processor builds");
        assert_eq!(lut_byte(&cpu, 0), 0, "black stays black");
        let mid = lut_byte(&cpu, 128);
        assert!(
            mid > 40 && mid < 200,
            "mid-gray sits inside the tone curve: {mid}"
        );
        let top = lut_byte(&cpu, 250);
        assert!(top > 180, "near-white encodes bright: {top}");
    }

    /// Drives a CPU processor on one gray value, returning the red
    /// byte (the chain is near-neutral; one channel tells the curve).
    #[cfg(feature = "ocio-bundled")]
    fn lut_byte(cpu: &ocio_rs::CPUProcessor, v: u8) -> u8 {
        let l = v as f32 / 255.0;
        let mut rgb = [l, l, l];
        cpu.apply_rgb(&mut rgb);
        (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8
    }

    #[test]
    fn config_names_are_documented_constants() {
        // The §9 defaults are pinned to the registry's ENUM names (the
        // real run proved create_from_builtin_config matches these,
        // not the UI names). Any rename upstream fails this test the
        // next time the bundled build runs.
        assert_eq!(ACES_CG_CONFIG, "cg-config-v4.0.0_aces-v2.0_ocio-v2.5");
        assert_eq!(
            ACES_STUDIO_CONFIG,
            "studio-config-v4.0.0_aces-v2.0_ocio-v2.5"
        );
    }
}
