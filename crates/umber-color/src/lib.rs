//! umber-color — color management integration.
//!
//! Wave 2 scope (docs/specs/requirements.md §9): OCIO v2 configs via
//! ocio-rs (vendored), scene-linear working space, display transforms via
//! GPU LUTs, per-channel CM flags (baseColor managed; roughness/normal = data).
//!
//! Wave 1 scope: the per-channel CM flag vocabulary.
//! Wave 2: the CPU display-transform reference path ([`display`]) — the
//! spec the GPU shader and the exporter validate against.
//! Wave 5: the viewer chain ([`DisplaySettings`] + [`apply_display_chain`])
//! the app's Display panel edits and the `.umber` project persists, and
//! [`build_display_lut`] — the chain tabulated for the GPU display LUT.

// Never-crash policy (clippy.toml + queue row 13, absorbed from the
// photocraft exploration): the engine crates deny unwrap/expect/panic
// in non-test code. The 2026-10-13 census found ZERO non-test uses,
// so this is a pin. Tests are exempt (the cfg_attr): the deny binds
// ONLY non-test builds, so test-code unwrap/expect stays legal while
// a shipped-code unwrap becomes a hard compile error.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod display;

#[cfg(feature = "ocio")]
pub mod ocio;

pub use display::{
    apply_display, apply_display_chain, build_display_lut, invert_display, DisplaySettings,
    DisplayTransform, DISPLAY_LUT_BYTES, DISPLAY_LUT_ENTRIES,
};

#[cfg(feature = "ocio")]
pub use display::{apply_display_chain_with, OcioDisplaySelection};

#[cfg(feature = "ocio")]
pub use ocio_rs;

#[cfg(feature = "ocio")]
pub use ocio::{
    builtin_configs, create_builtin_config, OcioBridgeError, OcioBuiltinConfig, ACES_CG_CONFIG,
    ACES_STUDIO_CONFIG,
};

/// Whether a channel's values are color-managed or raw data
/// (mirrors umber-core's ChannelKind; duplicated at the crate boundary
/// so umber-color stays standalone-usable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorManagement {
    /// sRGB-encoded, managed through OCIO on display.
    Managed,
    /// Linear data — displayed raw, exported without display transforms.
    Data,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cm_vocabulary() {
        assert_ne!(ColorManagement::Managed, ColorManagement::Data);
    }
}
