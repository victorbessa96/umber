//! umber-export — template/preset export engine.
//!
//! Wave 3 scope (docs/specs/requirements.md §6): naming tokens
//! ($mesh/$textureSet/$udim/$colorSpace/$srcMap/$layerName), channel
//! packing, engine presets (glTF/Unreal/Unity/Blender), normal-convention
//! conversion, PNG 8/16 + EXR 16F/32F + TIFF/JPEG.
//!
//! Wave 1 scope: the naming-token substitution engine — pure string
//! logic, fully testable headless, and the natural first real code here.
//! Wave 2 adds PNG encoding ([`png`]) of paint-target readbacks. Wave 3
//! adds EXR float export ([`exr`]) and TIFF/JPEG ([`formats`]).

pub mod dither;
pub mod driver;
pub mod exr;
pub mod formats;
pub mod icc;
pub mod png;
pub mod presets;
pub mod psd;
pub mod usd;

pub use dither::dither_quantize_rgba8;

pub use driver::{
    mesh_stem, output_color_space, run_preset, run_preset_tiled, run_preset_tiled_with_surface,
    run_preset_with_surface, src_map_token, tiled_template, ExportError, MapSet, TokenSources,
    DEFAULT_JPEG_QUALITY, SINGLE_TILE_UDIM, WORKING_NORMAL_CONVENTION,
};
pub use formats::{write_jpeg_rgba8, write_tiff_rgba8, JpegError, TiffError};
pub use icc::{embed_iccp, embed_srgb_iccp, SRGB_ICC_PROFILE};
pub use presets::{
    convert_normal, pack_texel, ChannelSlot, ExportPreset, ExportPresetError, MapKind,
    NormalConvention, OutputFormat, OutputSpec, Texel,
};
pub use umber_graph::mtlx_doc::SurfaceParams;

/// Substitute `$token` placeholders in an export path template.
///
/// Unknown tokens pass through verbatim so user templates never break
/// silently when a token's data is missing (documented, tested behavior).
///
/// Token VALUES are file-name-safe ([`path_safe_value`]): a `/` in the
/// TEMPLATE is the user's subfolder (Painter's convention), but a value
/// comes from the mesh file (material and layer names), and a crafted
/// `../../x` material must never write outside the output directory.
pub fn expand_template(template: &str, tokens: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in tokens {
        let key = format!("${}", name);
        if out.contains(&key) && !value.is_empty() {
            out = out.replace(&key, &path_safe_value(value));
        }
    }
    out
}

/// A token value made safe as ONE path component: path separators (`/`,
/// `\\`), the drive/stream colon, the other Windows-reserved characters
/// and control characters each become `_`. A value of only dots (`.`,
/// `..`) becomes underscores too, so it can never name a parent directory.
/// Ordinary names (`Body`, `Sword_Hilt`, `Metal 01`) pass unchanged.
pub fn path_safe_value(value: &str) -> String {
    let mapped: String = value
        .chars()
        .map(|c| match c {
            // Path separators, the drive/stream colon and Windows
            // wildcards are replaced; angle quotes stay (they cannot
            // climb a path, and `<UDIM>` is a standard token our own
            // tiled runs write — the CI-pinned name
            // `Sword_baseColor_<UDIM>.png` keeps its brackets).
            '/' | '\\' | ':' | '"' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    if !mapped.is_empty() && mapped.chars().all(|c| c == '.') {
        "_".repeat(mapped.len())
    } else {
        mapped
    }
}

/// Convert `/` separators in a template into subfolders (Painter
/// convention: slash creates directories).
pub fn template_to_path(template: &str) -> String {
    template.replace('/', std::path::MAIN_SEPARATOR_STR)
}

#[cfg(test)]
mod tests {

    #[test]
    fn token_values_cannot_climb_out_of_the_output_dir() {
        // A crafted mesh material name is a token VALUE: no separator,
        // drive colon or dot-only name survives into the path.
        for (value, expected) in [
            ("../../evil", ".._.._evil"),
            ("/etc/passwd", "_etc_passwd"),
            ("C:\\Windows\\x", "C__Windows_x"),
            ("..", "__"),
            (".", "_"),
            ("a\nb", "a_b"),
            ("Body", "Body"),
            ("Metal 01", "Metal 01"),
            // Brackets survive: no path can climb through them, and
            // `<UDIM>` is our own tiled-run token.
            ("Body_<UDIM>", "Body_<UDIM>"),
            ("Sword_Hilt.v2", "Sword_Hilt.v2"),
        ] {
            assert_eq!(path_safe_value(value), expected, "{value:?}");
        }
        // The template's own '/' still makes subfolders; a value's does not.
        let out = expand_template(
            "$mesh/$textureSet_color",
            &[("mesh", "sword"), ("textureSet", "../up")],
        );
        assert_eq!(out, "sword/.._up_color");
    }

    use super::*;

    #[test]
    fn expands_known_tokens() {
        let out = expand_template(
            "$mesh_$textureSet_$colorSpace",
            &[
                ("mesh", "sword"),
                ("textureSet", "Blade"),
                ("colorSpace", "basecolor"),
            ],
        );
        assert_eq!(out, "sword_Blade_basecolor");
    }

    #[test]
    fn unknown_tokens_pass_through() {
        let out = expand_template("$mesh_$unknown", &[("mesh", "sword")]);
        assert_eq!(out, "sword_$unknown");
    }

    #[test]
    fn empty_values_leave_placeholder() {
        let out = expand_template("$mesh_$udim", &[("mesh", "sword"), ("udim", "")]);
        assert_eq!(out, "sword_$udim");
    }

    #[test]
    fn template_separator_and_token_expansion_compose() {
        // template_to_path maps '/' to the platform separator (no-op on
        // Unix, '\' on Windows); token expansion composes after it. Assert
        // the composed contract against the platform's own separator.
        let pathed = template_to_path("out/$mesh/color");
        let expanded = expand_template(&pathed, &[("mesh", "sword")]);
        let expected = format!(
            "out{}sword{}color",
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR
        );
        assert_eq!(expanded, expected);
    }
}
