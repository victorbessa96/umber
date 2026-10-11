//! Inter (UI) + JetBrains Mono (numerics) — no system-font roulette
//! (docs/specs/gui-audit-and-roadmap.md §2.2).
//!
//! # Follow-up `font-assets` (the TTFs are not in the tree yet)
//!
//! Expected files, repo-relative:
//! - [`INTER_PATH`] = `assets/fonts/Inter-Regular.ttf` (Inter, OFL-1.1)
//! - [`MONO_PATH`] = `assets/fonts/JetBrainsMono-Regular.ttf` (OFL-1.1)
//!
//! Plus the two OFL license texts beside them. Once they land, release
//! builds enable the `embedded-fonts` feature, which bakes them in with
//! `include_bytes!`. Without that feature they load at runtime from the
//! fonts dir ([`fonts_dir`]). A missing file panics and names the path.
//! There is no silent fallback to egui's stock fonts.
//!
//! # Tabular numerals
//!
//! egui has no OpenType feature switch, so Inter's `tnum` cannot be
//! enabled. Value fields render in the Monospace family (JetBrains Mono),
//! where every digit has the same advance by construction ([`numeric_font`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

/// Repo-relative path of the UI font.
pub const INTER_PATH: &str = "assets/fonts/Inter-Regular.ttf";
/// Repo-relative path of the numeric/monospace font.
pub const MONO_PATH: &str = "assets/fonts/JetBrainsMono-Regular.ttf";
/// The `font_data` key for Inter.
pub const INTER_KEY: &str = "Inter";
/// The `font_data` key for JetBrains Mono.
pub const MONO_KEY: &str = "JetBrainsMono";
/// Env var that overrides the runtime fonts dir.
pub const FONTS_DIR_ENV: &str = "UMBER_FONTS_DIR";

/// The raw TTF bytes of the two families.
pub struct FontBytes {
    /// Inter Regular.
    pub inter: Vec<u8>,
    /// JetBrains Mono Regular.
    pub mono: Vec<u8>,
}

/// A font file that could not be read.
#[derive(Debug)]
pub struct MissingFont {
    /// The path that was tried.
    pub path: PathBuf,
    /// The I/O error message.
    pub reason: String,
}

impl std::fmt::Display for MissingFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "umber-gui: font file missing: {} ({}). Expected Inter at {INTER_PATH} and \
             JetBrains Mono at {MONO_PATH} (follow-up `font-assets`); set {FONTS_DIR_ENV} \
             to point at a different fonts dir",
            self.path.display(),
            self.reason
        )
    }
}

impl std::error::Error for MissingFont {}

/// The runtime fonts dir: `$UMBER_FONTS_DIR`, else the in-tree
/// `assets/fonts/` (resolved from this crate's manifest dir, so it is
/// correct for `cargo run` from a checkout. Packaged builds use
/// `embedded-fonts` or set the env var).
pub fn fonts_dir() -> PathBuf {
    match std::env::var_os(FONTS_DIR_ENV) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts"),
    }
}

fn read_font(dir: &Path, repo_path: &str) -> Result<Vec<u8>, MissingFont> {
    let file = Path::new(repo_path)
        .file_name()
        .expect("font path constants end in a file name");
    let path = dir.join(file);
    std::fs::read(&path).map_err(|e| MissingFont {
        path,
        reason: e.to_string(),
    })
}

/// Read both TTFs from `dir`.
pub fn load_font_bytes_from(dir: &Path) -> Result<FontBytes, MissingFont> {
    Ok(FontBytes {
        inter: read_font(dir, INTER_PATH)?,
        mono: read_font(dir, MONO_PATH)?,
    })
}

/// The font bytes: embedded with `embedded-fonts`, else read from
/// [`fonts_dir`].
#[cfg(not(feature = "embedded-fonts"))]
pub fn load_font_bytes() -> Result<FontBytes, MissingFont> {
    load_font_bytes_from(&fonts_dir())
}

/// The font bytes: embedded with `embedded-fonts`, else read from
/// [`fonts_dir`].
#[cfg(feature = "embedded-fonts")]
pub fn load_font_bytes() -> Result<FontBytes, MissingFont> {
    Ok(FontBytes {
        inter: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/fonts/Inter-Regular.ttf"
        ))
        .to_vec(),
        mono: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/fonts/JetBrainsMono-Regular.ttf"
        ))
        .to_vec(),
    })
}

/// Build the definitions from raw bytes: Inter first in Proportional,
/// JetBrains Mono first in Monospace. egui's stock fonts stay behind them
/// as glyph fallbacks (emoji, symbols), not as a replacement for a missing
/// file.
pub fn build_font_definitions(bytes: FontBytes) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    defs.font_data.insert(
        INTER_KEY.to_owned(),
        Arc::new(FontData::from_owned(bytes.inter)),
    );
    defs.font_data.insert(
        MONO_KEY.to_owned(),
        Arc::new(FontData::from_owned(bytes.mono)),
    );
    defs.families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, INTER_KEY.to_owned());
    defs.families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, MONO_KEY.to_owned());
    defs
}

/// The Umber font definitions. Panics with the expected paths when a
/// TTF is missing (see the module doc's `font-assets` follow-up).
pub fn umber_fonts() -> FontDefinitions {
    match load_font_bytes() {
        Ok(bytes) => build_font_definitions(bytes),
        Err(missing) => panic!("{missing}"),
    }
}

/// Install the Umber fonts on `ctx`. Panics on a missing TTF (see
/// [`umber_fonts`]).
pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(umber_fonts());
}

/// The tabular-numeral font for value fields (JetBrains Mono).
pub fn numeric_font(size: f32) -> FontId {
    FontId::monospace(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_puts_umber_fonts_first() {
        let defs = build_font_definitions(FontBytes {
            inter: vec![0; 4],
            mono: vec![1; 4],
        });
        assert_eq!(defs.families[&FontFamily::Proportional][0], INTER_KEY);
        assert_eq!(defs.families[&FontFamily::Monospace][0], MONO_KEY);
        assert!(defs.font_data.contains_key(INTER_KEY));
        assert!(defs.font_data.contains_key(MONO_KEY));
        // Stock fallbacks kept behind.
        assert!(defs.families[&FontFamily::Proportional].len() > 1);
    }

    #[test]
    fn missing_dir_names_the_expected_path() {
        let err = load_font_bytes_from(Path::new("/nonexistent/umber-fonts"))
            .err()
            .expect("no fonts in a nonexistent dir");
        assert!(err.path.ends_with("Inter-Regular.ttf"));
        let msg = err.to_string();
        assert!(msg.contains(INTER_PATH) && msg.contains(MONO_PATH));
    }
}
