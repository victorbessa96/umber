//! PSD import (flatten-on-import) — requirements §6 row 101.
//!
//! A dropped or chosen `.psd`/`.psb` becomes a texture set named for the
//! file; its flattened pixels land as the set's `baseColor`-equivalent
//! channel at the PSD's native resolution. Layer-map-to-texture-set-layers
//! (the row's full promise) is v2: named in the report, NOT attempted.
//!
//! # The honest subset (binding)
//!
//! * **Path A — the composite first.** The merged-image data Photoshop
//!   embeds (`Psd::canvas`, falling back to `Psd::image_data`) IS the
//!   flatten result, authored by Photoshop itself. When present and
//!   well-formed it is imported verbatim as the pixel truth.
//! * **Path B — the fallback rasterizer.** When the composite is absent
//!   (layered-only files, non-Photoshop authors) what can be rasterized
//!   honestly is: pixel layers in file order (bottom-to-top per the Adobe
//!   layer-record convention), group opacity as a group-wide alpha
//!   multiply, simple grayscale raster masks (1:1 texel mapping, no
//!   resampling), and the normal/multiply/screen blends. Anything else is
//!   flattened as normal and REPORTED — every deviation is a report line,
//!   never silent.
//! * **The import report** ([`PsdImportReport`]) is the same artifact class
//!   as the mesh-map importer's unmatched report: layers seen/imported,
//!   blend-mode substitutions, masks applied/skipped, groups, resolution,
//!   and color mode.
//!
//! # Judgment calls (v1 boundaries — read before extending)
//!
//! * **ag-psd API shape** (verified against the registry source
//!   `ag-psd-0.3.0`, not remembered APIs): `ag_psd::{read_psd}` +
//!   `ag_psd::psd::{Psd, Layer, PixelData, ColorMode, BlendMode,
//!   ReadOptions}`; `Psd::canvas` holds the composite under default read
//!   options (`reader.rs` only fills `image_data` with
//!   `use_image_data`); layer/mask canvases are RGBA8 `PixelData`
//!   (`image_data_to_canvas` is a clone); layer opacity is `0.0..=1.0`
//!   (`reader.rs` divides the byte by 255); mask density is `0.0..=1.0`
//!   and present only when the file stores non-default mask parameters;
//!   `children` preserves file order (end-to-start iteration with
//!   front-insert); a folder is a `Layer` with `children: Some(_)`
//!   (populated from section dividers).
//! * **z-order assumption.** `children` order is treated as bottom-to-top
//!   (Adobe's layer-record convention: bottom layer first in the file).
//!   The crate never states this; the two-layer test pins the
//!   implementation's self-consistency, and the report always carries a
//!   `z-order` note so a flipped real file is diagnosable, not silent.
//! * **Blend subset is normal/multiply/screen only**, via
//!   `umber_core::BlendMode::{Normal, Multiply, Screen}::blend_channel`
//!   — even though `umber-core` carries wider math (Darken, Subtract,
//!   Difference, …). The brief binds the three-mode subset; the wider
//!   math is v2's to claim, mode by mode, with verification each.
//! * **Bytes are blended as given (no transfer conversion).** The reader
//!   materializes RGBA8; blending those bytes matches Photoshop's legacy
//!   gamma-space compositing more closely than linearizing would.
//!   `umber-color` owns display transforms downstream, not import.
//! * **Color-mode conversions are reader-side.** 16/32-bit sources arrive
//!   downshifted to 8-bit (top byte), grayscale replicated to RGB,
//!   indexed expanded through the palette, CMYK *layers* through the
//!   reader's naive `(c·k)/255` inversion (no ICC). Each is a report note.
//!   CMYK/Lab/Multichannel/Duotone *composites* make `read_psd` itself
//!   fail (`StrictViolation: Color mode not supported`); those files are
//!   rejected as [`PsdImportError::UnsupportedColorMode`], never mangled.
//!   There is no umber-color CMYK path to route through (it owns display
//!   transforms, not import conversion) — the rejection names that.
//! * **Report strings are document data, not UI strings** (same rule as
//!   the mesh-map importer's `"Imported <map>"` layer names): they are
//!   plain English, NOT `tr()` keys. The dragon's menu/panel wiring
//!   i18n's the chrome around them; the per-file lines must read
//!   identically across locales. This keeps the i18n completeness test
//!   green with no `.ftl` delta from this kill.
//! * **No egui here.** The panel/menu wiring rides the wave-end merge
//!   (another claw owns the import-menu section this wave), so this
//!   module exposes the pure core + the [`PsdImportState`] drop/scan
//!   shape (mirroring `mesh_map_import`) with zero UI calls. An egui
//!   `show()` that nothing calls would be dead code under `-D warnings`.
//! * **Dims cap.** Sides `1..=8192`, total `<= 256 MiB` (an 8K RGBA8
//!   frame). Photoshop allows 30000px; the reader budgets 2 GiB. Above
//!   the cap is [`PsdImportError::InvalidDimensions`], not a 1 GiB
//!   allocation.
//! * **Malformed input is errors, never panics.** Every index is
//!   bounds-checked, every float finite-checked, every product
//!   checked-multiplied. Truncated headers and bogus layer counts
//!   surface as [`PsdImportError::Read`].

use std::path::{Path, PathBuf};

use ag_psd::psd::{BlendMode as PsdBlendMode, ColorMode, Layer as PsdLayer, Psd, ReadOptions};
use ag_psd::read_psd;
use umber_core::BlendMode as UmberBlendMode;

/// File extensions this importer reads (case-insensitive, like the mesh
/// drop router's extension checks).
pub const PSD_EXTENSIONS: [&str; 2] = ["psd", "psb"];

/// The channel flattened PSD pixels land in (mirrors
/// `umber_core::TextureSet::new_default`'s first channel; kept as a local
/// const because the Document/paint wiring lands with the menu merge).
pub const PSD_TARGET_CHANNEL: &str = "baseColor";

/// Longest allowed PSD side, pixels (see the module docs).
pub const MAX_PSD_SIDE: u32 = 8192;
/// Largest allowed flattened frame, bytes (exactly one 8K RGBA8 frame).
pub const MAX_PSD_BYTES: usize = 256 * 1024 * 1024;
/// The decode budget handed to ag-psd: four flattened frames (the
/// composite plus layer canvases), 1 GiB, half ag-psd's 2 GiB default.
pub const PSD_DECODE_BUDGET: usize = 4 * MAX_PSD_BYTES;
/// Largest `.psd`/`.psb` read from disk (2 GiB, Photoshop's PSD maximum;
/// PSB can be larger and is refused here).
pub const MAX_PSD_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Whether `path` is a PSD/PSB file (case-insensitive extension) — the
/// predicate the shell's drop router will use beside
/// `empty_state::is_mesh_file` (wired at the merge; pure here).
pub fn is_psd_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            PSD_EXTENSIONS
                .iter()
                .any(|want| ext.eq_ignore_ascii_case(want))
        })
}

/// The texture-set name for an imported file: its stem (`Sword.psd` →
/// `Sword`). Falls back to `"TextureSet"` when the stem is missing.
pub fn set_name_from_file(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    if stem.is_empty() {
        "TextureSet".to_string()
    } else {
        stem.to_string()
    }
}

/// Everything that can go wrong importing one PSD.
#[derive(Debug, thiserror::Error)]
pub enum PsdImportError {
    /// `read_psd` failed (truncated bytes, bogus counts, bad compression,
    /// over the reader's memory budget, …). The reader's message is kept
    /// verbatim in `detail`.
    #[error("could not read PSD: {detail}")]
    Read {
        /// The reader's failure, verbatim.
        detail: String,
    },
    /// The file declares a color mode whose composite the reader cannot
    /// decode (CMYK/Lab/Multichannel/Duotone — `StrictViolation: Color
    /// mode not supported` in `reader.rs`). Rejected, never mangled.
    #[error("unsupported PSD color mode {mode}: the reader decodes RGB/grayscale/indexed/bitmap composites only")]
    UnsupportedColorMode {
        /// The declared mode (`Cmyk`, `Lab`, …).
        mode: String,
    },
    /// The document or composite dimensions are unusable (non-finite,
    /// zero, fractional, over [`MAX_PSD_SIDE`], or overflowing the pixel
    /// buffer even under [`MAX_PSD_BYTES`]).
    #[error("unusable PSD dimensions: {detail}")]
    InvalidDimensions {
        /// What was declared and which bound it broke.
        detail: String,
    },
    /// No pixel truth exists: no composite AND no rasterizable layer (an
    /// empty document, or one holding only hidden/adjustment layers).
    #[error("no pixel truth in PSD: {detail}")]
    NoPixelTruth {
        /// The layer/group census behind the refusal.
        detail: String,
    },
}

/// Which flatten path produced the pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlattenPath {
    /// The embedded composite, authored by Photoshop itself (highest
    /// fidelity, lowest risk).
    #[default]
    Composite,
    /// The honest fallback rasterizer (see the module docs).
    FallbackRaster,
}

/// The import report: every deviation from the true composite is a line
/// here, never silent. Document data (plain English — see the module
/// docs), rendered by the future panel via [`PsdImportReport::summary`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PsdImportReport {
    /// The file as presented (name only, not the full path).
    pub source_file: String,
    /// The texture set the pixels belong to ([`set_name_from_file`]).
    pub set_name: String,
    /// Native resolution the pixels landed at.
    pub width: u32,
    /// Native resolution the pixels landed at.
    pub height: u32,
    /// Declared color mode (`Rgb`, `Grayscale`, `Cmyk`, …; `unspecified`
    /// when the header leaves it blank and the reader assumes Rgb).
    pub color_mode: String,
    /// Reader-side conversions applied before flattening (16-bit
    /// downshift, grayscale replication, …). Empty for plain RGB8.
    pub conversions: Vec<String>,
    /// Which path produced the pixels.
    pub path: FlattenPath,
    /// Every layer/group record walked (pixel layers + folders).
    pub layers_seen: usize,
    /// Pixel layers actually composited.
    pub pixel_layers: usize,
    /// Folders (group records) walked.
    pub groups_seen: usize,
    /// One line per blend flattened as something it is not
    /// (`"<mode>" on "<layer>" flattened as normal`, plus group and
    /// clipping deviations).
    pub substitutions: Vec<String>,
    /// One line per mask that contributed (`"<layer>" (user mask)`).
    pub masks_applied: Vec<String>,
    /// One line per mask that did not (`"<layer>": <reason>`).
    pub masks_skipped: Vec<String>,
    /// Everything else worth knowing: z-order assumption, hidden/empty
    /// skips, geometry clips, resolution, and the v2 layer-mapping
    /// deferral.
    pub notes: Vec<String>,
}

impl PsdImportReport {
    /// Deterministic one-line-per-fact rendering for the future panel and
    /// the logs. Order is fixed (identity first, then counts, then the
    /// per-layer lines in walk order) so golden tests can pin it.
    pub fn summary(&self) -> Vec<String> {
        let mut lines = vec![
            format!(
                "psd {} -> set {} ({}x{}, {})",
                self.source_file, self.set_name, self.width, self.height, self.color_mode
            ),
            format!(
                "path: {}",
                match self.path {
                    FlattenPath::Composite => "composite (Photoshop-authored)",
                    FlattenPath::FallbackRaster => "fallback rasterizer (no composite)",
                }
            ),
            format!(
                "layers: {} seen, {} pixel composited, {} groups",
                self.layers_seen, self.pixel_layers, self.groups_seen
            ),
        ];
        for conversion in &self.conversions {
            lines.push(format!("conversion: {conversion}"));
        }
        for substitution in &self.substitutions {
            lines.push(format!("substitution: {substitution}"));
        }
        for applied in &self.masks_applied {
            lines.push(format!("mask applied: {applied}"));
        }
        for skipped in &self.masks_skipped {
            lines.push(format!("mask skipped: {skipped}"));
        }
        for note in &self.notes {
            lines.push(format!("note: {note}"));
        }
        lines.push(
            "layer-map-to-texture-set-layers is v2 (deferred): this import lands one flattened baseColor truth, not per-layer texture-set layers"
                .to_string(),
        );
        lines
    }
}

/// One successful import: the flattened truth plus its report. The pixels
/// are RGBA8 row-major at (`width`, `height`) — the set's
/// [`PSD_TARGET_CHANNEL`] content at the PSD's native resolution.
#[derive(Debug, Clone)]
pub struct PsdImport {
    /// The texture set the pixels belong to.
    pub set_name: String,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// RGBA8 row-major pixels (`width * height * 4` bytes).
    pub pixels: Vec<u8>,
    /// The honest-verification artifact.
    pub report: PsdImportReport,
}

/// Reads `bytes` (named `file_name`, used for the set name and report)
/// and flattens them: composite first ([`FlattenPath::Composite`]), the
/// fallback rasterizer when it is absent ([`FlattenPath::FallbackRaster`]).
pub fn import_psd_bytes(bytes: &[u8], file_name: &str) -> Result<PsdImport, PsdImportError> {
    // ag-psd's default budget is 2 GiB of decoded pixels; this import
    // keeps one flattened frame of at most MAX_PSD_BYTES (256 MiB), so a
    // layered file is given that frame plus the same again for layers,
    // and a hostile file fails typed at 1 GiB instead of 2.
    let options = ReadOptions {
        total_memory_limit: Some(PSD_DECODE_BUDGET),
        ..ReadOptions::default()
    };
    let detail = match read_psd(bytes, &options) {
        Ok(psd) => return flatten_psd(&psd, file_name),
        Err(err) => err.to_string(),
    };
    Err(map_read_error(&detail, bytes))
}

/// Maps the reader's failure to the honest error: files declaring a
/// composite-undecodable color mode become
/// [`PsdImportError::UnsupportedColorMode`]; everything else stays a
/// [`PsdImportError::Read`] with the message verbatim. The sniff reads
/// the 26-byte header only (bounds-checked — short input yields `None`).
pub fn map_read_error(detail: &str, bytes: &[u8]) -> PsdImportError {
    if let Some(mode) = sniff_color_mode(bytes) {
        if matches!(mode.as_str(), "Cmyk" | "Lab" | "Multichannel" | "Duotone") {
            return PsdImportError::UnsupportedColorMode { mode };
        }
    }
    PsdImportError::Read {
        detail: detail.to_string(),
    }
}

/// Peeks the PSD header's declared color mode: 4-byte `8BPS` signature,
/// version 1 (PSD) or 2 (PSB), then the mode `u16` at offset 24
/// (4 sig + 2 version + 6 reserved + 2 channels + 4 height + 4 width + 2
/// depth). `None` when the header is unreadable (truncated/garbage).
pub fn sniff_color_mode(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 26 || &bytes[0..4] != b"8BPS" {
        return None;
    }
    let version = u16::from_be_bytes([bytes[4], bytes[5]]);
    if version != 1 && version != 2 {
        return None;
    }
    let mode = u16::from_be_bytes([bytes[24], bytes[25]]);
    let name = match mode {
        0 => "Bitmap",
        1 => "Grayscale",
        2 => "Indexed",
        3 => "Rgb",
        4 => "Cmyk",
        7 => "Multichannel",
        8 => "Duotone",
        9 => "Lab",
        other => return Some(format!("unknown({other})")),
    };
    Some(name.to_string())
}

/// Flattens an already-parsed document (pure over `psd`, so the fallback
/// rasterizer is unit-testable with hand-built structs — no files).
/// `file_name` feeds the set name and the report only.
pub fn flatten_psd(psd: &Psd, file_name: &str) -> Result<PsdImport, PsdImportError> {
    let set_name = set_name_from_file(file_name);
    let (doc_width, doc_height) = psd_dimensions(psd)?;
    let mut report = PsdImportReport {
        source_file: file_name.to_string(),
        set_name: set_name.clone(),
        width: doc_width,
        height: doc_height,
        color_mode: color_mode_name(psd.color_mode),
        ..Default::default()
    };
    report.conversions = conversion_notes(psd);

    // Path A: the Photoshop-authored composite, verbatim.
    if let Some((pixels, used_image_data)) = composite_pixels(psd) {
        report.path = FlattenPath::Composite;
        report.width = pixels.0;
        report.height = pixels.1;
        let layer_count = psd.children.as_ref().map_or(0, Vec::len);
        report.layers_seen = layer_count;
        report.notes.push(format!(
            "composite from {} ({} layers present, mapped to one flattened truth)",
            if used_image_data {
                "image_data"
            } else {
                "canvas"
            },
            layer_count
        ));
        report.notes.push(format!(
            "landed as {PSD_TARGET_CHANNEL} at native {}x{}",
            pixels.0, pixels.1
        ));
        return Ok(PsdImport {
            set_name,
            width: pixels.0,
            height: pixels.1,
            pixels: pixels.2,
            report,
        });
    }
    if psd.canvas.is_some() || psd.image_data.is_some() {
        report.notes.push(
            "embedded composite present but malformed (size mismatch) — fell back to layers"
                .to_string(),
        );
    } else {
        report.notes.push(
            "no embedded composite (layered-only or non-Photoshop author) — fell back to layers"
                .to_string(),
        );
    }
    report.path = FlattenPath::FallbackRaster;

    // Path B: the honest fallback rasterizer.
    let total = (doc_width as usize)
        .checked_mul(doc_height as usize)
        .and_then(|pixels| pixels.checked_mul(4));
    let Some(total) = total else {
        return Err(PsdImportError::InvalidDimensions {
            detail: format!("{doc_width}x{doc_height} overflows the pixel buffer"),
        });
    };
    if total > MAX_PSD_BYTES {
        return Err(PsdImportError::InvalidDimensions {
            detail: format!("{doc_width}x{doc_height} exceeds the 256 MiB frame cap"),
        });
    }
    let mut acc = vec![0u8; total];
    let size = ImageSize {
        width: doc_width,
        height: doc_height,
    };
    let mut ctx = FlattenCtx::default();
    if let Some(children) = psd.children.as_ref() {
        walk_nodes(children, &mut acc, size, 1.0, &mut ctx);
    }
    report.layers_seen = ctx.layers_seen;
    report.pixel_layers = ctx.pixel_layers;
    report.groups_seen = ctx.groups_seen;
    report.substitutions = ctx.substitutions;
    report.masks_applied = ctx.masks_applied;
    report.masks_skipped = ctx.masks_skipped;
    report.notes.extend(ctx.notes);
    report.notes.push(
        "z-order: file order composited bottom-to-top (Adobe layer-record convention)".to_string(),
    );
    report.notes.push(format!(
        "landed as {PSD_TARGET_CHANNEL} at native {doc_width}x{doc_height}"
    ));
    if ctx.pixel_layers == 0 {
        let detail = format!(
            "{} records seen ({} groups, {} hidden/skipped)",
            ctx.layers_seen, ctx.groups_seen, ctx.skipped
        );
        return Err(PsdImportError::NoPixelTruth { detail });
    }
    Ok(PsdImport {
        set_name,
        width: doc_width,
        height: doc_height,
        pixels: acc,
        report,
    })
}

/// The document dimensions, checked (finite, integral, `1..=MAX_PSD_SIDE`).
fn psd_dimensions(psd: &Psd) -> Result<(u32, u32), PsdImportError> {
    let convert = |value: f64, what: &str| {
        if !value.is_finite() || value.fract() != 0.0 {
            return Err(what.to_string());
        }
        if value < 1.0 || value > f64::from(MAX_PSD_SIDE) {
            return Err(what.to_string());
        }
        Ok(value as u32)
    };
    let width = convert(psd.width, "width").map_err(|what| PsdImportError::InvalidDimensions {
        detail: format!("{what} {} out of 1..={}", psd.width, MAX_PSD_SIDE),
    })?;
    let height =
        convert(psd.height, "height").map_err(|what| PsdImportError::InvalidDimensions {
            detail: format!("{what} {} out of 1..={}", psd.height, MAX_PSD_SIDE),
        })?;
    Ok((width, height))
}

/// The declared color-mode name (`unspecified` mirrors the reader's
/// `unwrap_or(ColorMode::Rgb)` default).
fn color_mode_name(mode: Option<ColorMode>) -> String {
    match mode {
        None => "unspecified (assumed Rgb)".to_string(),
        Some(ColorMode::Bitmap) => "Bitmap".to_string(),
        Some(ColorMode::Grayscale) => "Grayscale".to_string(),
        Some(ColorMode::Indexed) => "Indexed".to_string(),
        Some(ColorMode::Rgb) => "Rgb".to_string(),
        Some(ColorMode::Cmyk) => "Cmyk".to_string(),
        Some(ColorMode::Multichannel) => "Multichannel".to_string(),
        Some(ColorMode::Duotone) => "Duotone".to_string(),
        Some(ColorMode::Lab) => "Lab".to_string(),
    }
}

/// Reader-side conversions the flatten inherits (each becomes a report
/// note — conversions are reported, not hidden).
fn conversion_notes(psd: &Psd) -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(bits) = psd.bits_per_channel.filter(|bits| *bits != 8.0) {
        notes.push(format!(
            "{bits}-bit source downshifted to 8-bit by the reader (top byte)"
        ));
    }
    match psd.color_mode {
        Some(ColorMode::Grayscale) => {
            notes.push("grayscale replicated to RGB by the reader".to_string());
        }
        Some(ColorMode::Indexed) => {
            notes.push("indexed expanded through the palette by the reader".to_string());
        }
        Some(ColorMode::Bitmap) => {
            notes.push("1-bit bitmap expanded by the reader".to_string());
        }
        Some(ColorMode::Cmyk) => {
            notes.push("Cmyk layer pixels via the reader's naive inversion (no ICC)".to_string());
        }
        _ => {}
    }
    notes
}

/// The embedded composite when present AND well-formed: non-empty data of
/// exactly `width * height * 4` bytes with nonzero dims. Returns the
/// pixels plus whether they came from `image_data` (`canvas` otherwise).
/// The composite's truth: (width, height, RGBA8 bytes), plus whether
/// the pixels carry meaningful alpha (Photoshop's flattened composite
/// is opaque — `false` means the data came from a composite).
type Composite = ((u32, u32, Vec<u8>), bool);

fn composite_pixels(psd: &Psd) -> Option<Composite> {
    let (data, used_image_data) = match (psd.canvas.as_ref(), psd.image_data.as_ref()) {
        (Some(canvas), _) => (canvas, false),
        (None, Some(image)) => (image, true),
        (None, None) => return None,
    };
    if data.width == 0 || data.height == 0 || data.data.is_empty() {
        return None;
    }
    let total = (data.width as usize)
        .checked_mul(data.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))?;
    if data.data.len() != total || total > MAX_PSD_BYTES {
        return None;
    }
    Some((
        (data.width, data.height, data.data.clone()),
        used_image_data,
    ))
}

/// Canvas dimensions for the fallback walk.
#[derive(Debug, Clone, Copy)]
struct ImageSize {
    width: u32,
    height: u32,
}

/// One layer's integer rect: its canvas covers
/// (`x`, `y`)..(`x`+`w`, `y`+`h`) in document pixels.
#[derive(Debug, Clone, Copy)]
struct LayerRect {
    x: i64,
    y: i64,
    w: u32,
    h: u32,
}

/// The walk's mutable census + report lines (one struct: the walk takes
/// six params, staying under clippy's seven-arg limit).
#[derive(Debug, Default)]
struct FlattenCtx {
    layers_seen: usize,
    pixel_layers: usize,
    groups_seen: usize,
    skipped: usize,
    substitutions: Vec<String>,
    masks_applied: Vec<String>,
    masks_skipped: Vec<String>,
    notes: Vec<String>,
}

/// A layer's integer rect, or `None` when the record is non-finite,
/// inverted, or empty (adjustment/folder records legitimately have none).
fn layer_rect(layer: &PsdLayer) -> Option<LayerRect> {
    let left = layer.left.unwrap_or(0.0);
    let top = layer.top.unwrap_or(0.0);
    let right = layer.right.unwrap_or(0.0);
    let bottom = layer.bottom.unwrap_or(0.0);
    if !left.is_finite() || !top.is_finite() || !right.is_finite() || !bottom.is_finite() {
        return None;
    }
    let w = (right - left).round();
    let h = (bottom - top).round();
    if w < 1.0 || h < 1.0 || w > f64::from(MAX_PSD_SIDE) || h > f64::from(MAX_PSD_SIDE) {
        return None;
    }
    Some(LayerRect {
        x: left.round() as i64,
        y: top.round() as i64,
        w: w as u32,
        h: h as u32,
    })
}

/// Display name for walk/report lines (record names are document data —
/// used verbatim, never translated).
fn layer_name(layer: &PsdLayer, index: usize) -> String {
    layer
        .additional_info
        .name
        .clone()
        .unwrap_or_else(|| format!("layer {index}"))
}

/// A record's effective opacity: missing means 1.0 (the writer's
/// `unwrap_or(1.0)`); non-finite fails closed to 0.0 like
/// `BlendMode::clamp01`.
fn layer_opacity(opacity: Option<f64>) -> f32 {
    match opacity {
        None => 1.0,
        Some(value) => {
            if value.is_finite() {
                value.clamp(0.0, 1.0) as f32
            } else {
                0.0
            }
        }
    }
}

/// The honest blend subset: normal/multiply/screen are exact (via
/// `umber_core`'s verified math); every other mode returns `Normal` plus
/// its substitution name for the report.
fn honest_blend(mode: Option<PsdBlendMode>) -> (UmberBlendMode, Option<String>) {
    match mode {
        None | Some(PsdBlendMode::Normal) => (UmberBlendMode::Normal, None),
        Some(PsdBlendMode::Multiply) => (UmberBlendMode::Multiply, None),
        Some(PsdBlendMode::Screen) => (UmberBlendMode::Screen, None),
        Some(other) => (UmberBlendMode::Normal, Some(format!("{other:?}"))),
    }
}

/// How one layer's mask contributes (see the module docs for what counts
/// as a "simple grayscale raster").
enum MaskSample {
    /// No mask record, or a uniform full-reveal default.
    None,
    /// A uniform multiplier (mask record with `default_color` but no
    /// raster).
    Uniform(f32),
    /// A 1:1 raster placed at (`ox`, `oy`); out-of-rect samples read
    /// `outside`; everything scales by `density`.
    Raster {
        data: Vec<u8>,
        width: u32,
        height: u32,
        ox: i64,
        oy: i64,
        outside: f32,
        density: f32,
    },
}

/// Resolves one layer's mask to a [`MaskSample`], recording applied/skipped
/// lines. `mask` (user mask) wins over `real_mask`; feathered or disabled
/// masks are skipped, never approximated.
fn mask_sample(layer: &PsdLayer, name: &str, ctx: &mut FlattenCtx) -> MaskSample {
    let info = &layer.additional_info;
    let (mask, which) = match (info.mask.as_ref(), info.real_mask.as_ref()) {
        (Some(mask), _) => (mask, "user mask"),
        (None, Some(mask)) => (mask, "real mask"),
        (None, None) => return MaskSample::None,
    };
    if info.mask.is_some() && info.real_mask.is_some() {
        ctx.notes.push(format!(
            "{name}: both user and real masks present — user mask wins"
        ));
    }
    if mask.disabled == Some(true) {
        ctx.masks_skipped.push(format!("{name} ({which} disabled)"));
        return MaskSample::None;
    }
    if let Some(feather) = mask.user_mask_feather {
        if !feather.is_finite() || feather != 0.0 {
            ctx.masks_skipped.push(format!(
                "{name} ({which} feather {feather} not implemented)"
            ));
            return MaskSample::None;
        }
    }
    let density = match mask.user_mask_density {
        None => 1.0,
        Some(value) if value.is_finite() => value.clamp(0.0, 1.0) as f32,
        Some(_) => 1.0,
    };
    let canvas = mask.canvas.as_ref().or(mask.image_data.as_ref());
    let Some(canvas) = canvas else {
        // No raster: the record's default color is the uniform mask
        // (Photoshop: 0 hides, 255 reveals; absent means reveal).
        let uniform = match mask.default_color {
            Some(value) if value.is_finite() => (value.clamp(0.0, 255.0) / 255.0) as f32,
            _ => 1.0,
        };
        if uniform >= 1.0 {
            return MaskSample::None;
        }
        ctx.masks_applied
            .push(format!("{name} ({which} uniform default)"));
        return MaskSample::Uniform(uniform * density);
    };
    if canvas.data.is_empty() {
        ctx.masks_skipped
            .push(format!("{name} ({which} has no raster)"));
        return MaskSample::None;
    }
    let ox = mask.left.unwrap_or(0.0);
    let oy = mask.top.unwrap_or(0.0);
    if !ox.is_finite() || !oy.is_finite() {
        ctx.masks_skipped
            .push(format!("{name} ({which} has a non-finite origin)"));
        return MaskSample::None;
    }
    let outside = match mask.default_color {
        Some(value) if value.is_finite() => (value.clamp(0.0, 255.0) / 255.0) as f32,
        _ => 1.0,
    };
    ctx.masks_applied.push(format!("{name} ({which})"));
    if density < 1.0 {
        ctx.notes.push(format!(
            "{name}: mask density {density} applied as a global multiply"
        ));
    }
    MaskSample::Raster {
        data: canvas.data.clone(),
        width: canvas.width,
        height: canvas.height,
        ox: ox.round() as i64,
        oy: oy.round() as i64,
        outside,
        density,
    }
}

/// Samples a resolved mask at document pixel (`dx`, `dy`) as a `0..=1`
/// multiplier.
fn sample_mask(mask: &MaskSample, dx: i64, dy: i64) -> f32 {
    match mask {
        MaskSample::None => 1.0,
        MaskSample::Uniform(value) => *value,
        MaskSample::Raster {
            data,
            width,
            height,
            ox,
            oy,
            outside,
            density,
        } => {
            let mx = dx - *ox;
            let my = dy - *oy;
            let value = if mx >= 0 && my >= 0 {
                let (mx, my) = (mx as u64, my as u64);
                if mx < u64::from(*width) && my < u64::from(*height) {
                    let at = (my * u64::from(*width) + mx) as usize * 4;
                    // The reader decodes mask channels into R (G/B/A stay
                    // zero); synthetic fixtures set R=G=B alike.
                    data.get(at)
                        .map_or(*outside, |byte| f32::from(*byte) / 255.0)
                } else {
                    *outside
                }
            } else {
                *outside
            };
            value * density
        }
    }
}

/// Blends one source texel over one destination texel: the honest mode's
/// RGB blend, then Porter-Duff `over` with coverage `src_alpha * mask *
/// opacity`. Operates on bytes-as-given (see the module docs).
fn over_pixel(dst: &mut [u8], src: &[u8], mode: UmberBlendMode, coverage: f32) {
    if coverage <= 0.0 || src.len() < 4 || dst.len() < 4 {
        return;
    }
    let src_a = f32::from(src[3]) / 255.0;
    let dst_a = f32::from(dst[3]) / 255.0;
    let sa = (src_a * coverage).clamp(0.0, 1.0);
    if sa <= 0.0 {
        return;
    }
    let out_a = sa + dst_a * (1.0 - sa);
    for channel in 0..3 {
        let s = f32::from(src[channel]) / 255.0;
        let d = f32::from(dst[channel]) / 255.0;
        // The three mapped modes never return `None`; the fallback keeps
        // totality if `umber-core` ever narrows them.
        let blended = mode.blend_channel(s, d).unwrap_or(s);
        let value = blended * sa + d * (1.0 - sa);
        dst[channel] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// Counts the records under `nodes` (for the hidden-group skip line).
fn count_records(nodes: &[PsdLayer]) -> usize {
    let mut count = 0;
    for node in nodes {
        count += 1;
        if let Some(children) = node.children.as_ref() {
            count += count_records(children);
        }
    }
    count
}

/// Composites `nodes` (file order = bottom-to-top) over `acc`.
/// `chain` is the accumulated group-opacity multiply from enclosing
/// folders.
fn walk_nodes(
    nodes: &[PsdLayer],
    acc: &mut [u8],
    size: ImageSize,
    chain: f32,
    ctx: &mut FlattenCtx,
) {
    for (index, node) in nodes.iter().enumerate() {
        let name = layer_name(node, index);
        ctx.layers_seen += 1;

        // A folder: recurse with the group-wide multiply.
        if let Some(children) = node.children.as_ref() {
            ctx.groups_seen += 1;
            if node.hidden == Some(true) {
                let skipped = count_records(children);
                ctx.skipped += skipped;
                ctx.notes
                    .push(format!("group {name} hidden — {skipped} records skipped"));
                continue;
            }
            let group_opacity = layer_opacity(node.opacity);
            if group_opacity < 1.0 {
                ctx.notes.push(format!(
                    "group {name} opacity {group_opacity} applied as a group-wide multiply"
                ));
            }
            match node.blend_mode {
                None | Some(PsdBlendMode::PassThrough) => {}
                Some(PsdBlendMode::Normal) => {
                    ctx.substitutions.push(format!(
                        "\"Normal\" on group {name} treated as passthrough-with-multiply (no isolation)"
                    ));
                }
                Some(other) => {
                    ctx.substitutions.push(format!(
                        "\"{other:?}\" on group {name} flattened as passthrough-with-multiply"
                    ));
                }
            }
            if node.clipping == Some(true) {
                ctx.substitutions.push(format!(
                    "clipping on group {name} ignored (flattened as unclipped)"
                ));
            }
            walk_nodes(children, acc, size, chain * group_opacity, ctx);
            continue;
        }

        if node.hidden == Some(true) {
            ctx.skipped += 1;
            ctx.notes.push(format!("layer {name} hidden — skipped"));
            continue;
        }
        let canvas = node.canvas.as_ref().or(node.image_data.as_ref());
        let Some(canvas) = canvas else {
            ctx.skipped += 1;
            let mut why = format!("layer {name} has no raster pixels");
            if node.additional_info.adjustment.is_some() {
                why.push_str(" (adjustment layer)");
            } else if node.additional_info.text.is_some() {
                why.push_str(" (text layer)");
            }
            ctx.notes.push(why);
            continue;
        };
        let Some(rect) = layer_rect(node) else {
            ctx.skipped += 1;
            ctx.notes.push(format!(
                "layer {name} has an empty/non-finite rect — skipped"
            ));
            continue;
        };
        if canvas.width != rect.w || canvas.height != rect.h {
            ctx.skipped += 1;
            ctx.notes.push(format!(
                "layer {name} canvas {}x{} disagrees with its rect {}x{} — skipped",
                canvas.width, canvas.height, rect.w, rect.h
            ));
            continue;
        }
        if canvas.data.len() != rect.w as usize * rect.h as usize * 4 {
            ctx.skipped += 1;
            ctx.notes
                .push(format!("layer {name} canvas length mismatch — skipped"));
            continue;
        }

        let (mode, substituted) = honest_blend(node.blend_mode);
        if let Some(sub) = substituted {
            ctx.substitutions
                .push(format!("\"{sub}\" on layer {name} flattened as normal"));
        }
        if node.clipping == Some(true) {
            ctx.substitutions.push(format!(
                "clipping on layer {name} ignored (flattened as unclipped)"
            ));
        }
        if node.additional_info.blending_ranges.is_some() {
            ctx.notes
                .push(format!("layer {name}: blend-if ranges ignored"));
        }
        if node.additional_info.knockout == Some(true) {
            ctx.notes.push(format!("layer {name}: knockout ignored"));
        }
        if node.additional_info.vector_mask.is_some() {
            ctx.notes.push(format!(
                "layer {name}: vector mask present — raster mask only"
            ));
        }
        let mut opacity = layer_opacity(node.opacity) * chain;
        if let Some(fill) = node.additional_info.fill_opacity {
            if fill.is_finite() {
                opacity *= fill.clamp(0.0, 1.0) as f32;
                ctx.notes.push(format!(
                    "layer {name}: fill-opacity {fill} folded into opacity"
                ));
            }
        }
        let mask = mask_sample(node, &name, ctx);

        let mut clipped = false;
        for (row, src_row) in canvas.data.chunks_exact(rect.w as usize * 4).enumerate() {
            let dy = rect.y + row as i64;
            if dy < 0 || dy >= i64::from(size.height) {
                clipped = true;
                continue;
            }
            for (col, src_px) in src_row.chunks_exact(4).enumerate() {
                let dx = rect.x + col as i64;
                if dx < 0 || dx >= i64::from(size.width) {
                    clipped = true;
                    continue;
                }
                let coverage = opacity * sample_mask(&mask, dx, dy);
                let at = (dy as usize * size.width as usize + dx as usize) * 4;
                if let Some(dst_px) = acc.get_mut(at..at + 4) {
                    over_pixel(dst_px, src_px, mode, coverage);
                } else {
                    clipped = true;
                }
            }
        }
        if clipped {
            ctx.notes
                .push(format!("layer {name} extends past the canvas — clipped"));
        }
        ctx.pixel_layers += 1;
    }
}

/// Why a file did not enter the pending queue (the unmatched-report row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsdSkippedReason {
    /// Not a `.psd`/`.psb` file (the scan reports, never fails).
    NotPsd,
}

impl PsdSkippedReason {
    /// The future panel's i18n key (reserved for the menu wiring — no
    /// `tr()` call here, so the completeness test stays green).
    pub fn key(self) -> &'static str {
        match self {
            Self::NotPsd => "psd-import.reason-not-psd",
        }
    }
}

/// One routed file waiting for import (scan output, pre-flatten).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPsd {
    /// The file name as presented.
    pub file: String,
    /// Full path when the scan saw the filesystem (`None` for
    /// name-only fixtures).
    pub path: Option<PathBuf>,
    /// The texture set it will become ([`set_name_from_file`]).
    pub set_name: String,
}

/// One skipped file for the unmatched report row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmatchedPsd {
    /// The file name as presented.
    pub file: String,
    /// Why it did not route.
    pub reason: PsdSkippedReason,
}

/// Publishes a flattened PSD into paint-plane bytes (the C2-32 rule
/// for PSD, named in `2026-10-16-import-srgb-linearize`'s follow-ups):
/// the reader's pixels are Photoshop's gamma-space composite (the
/// module's blend rule keeps bytes as given), and the planes are
/// LINEAR — so RGB linearizes through the shared sRGB→linear curve
/// BEFORE the premultiply, alpha untouched. A PSD flat is always a
/// colour image (there is no per-channel kind to check, unlike the
/// mesh-map importer's `channel_kind_for`).
///
/// Pure: bytes in, bytes out. The shell (main.rs, lock-queued) binds
/// this to `PaintState::publish_plane` at tile 1001 (PSDs carry no
/// UDIM) with the set's `PsdImport::set_name`.
/// (Shell-half seam: the W-GUI-3 PSD drop-binding kill wires this to
/// `publish_plane` at tile 1001; unwired until then.)
#[allow(dead_code)]
pub fn publish_psd_pixels(flat: &PsdImport) -> Vec<u8> {
    let mut rgba = flat.pixels.clone();
    umber_paint::paint_state::curvature_png_to_linear(&mut rgba);
    umber_paint::material_layer::straight_to_premultiplied(&rgba)
}

/// The folder-drop UX state, mirroring `mesh_map_import`'s shape: the
/// last scan's routed files, the unmatched report, the session set name
/// the apply path retargets, and the last import's report. The dragon's
/// menu wiring drives this from the Assets panel + shell drops.
#[derive(Debug)]
pub struct PsdImportState {
    /// Routed files from the last scan / drop, in scan order.
    pub pending: Vec<PendingPsd>,
    /// Skipped files from the last scan / drop, in scan order.
    pub skipped: Vec<UnmatchedPsd>,
    /// How many files have been flattened this session.
    pub applied: usize,
    /// The session texture-set name (follows the mesh / project via
    /// [`Self::retarget_set`]).
    pub session_set_name: String,
    /// The last import's report (panel + log rendering).
    pub last_report: Option<PsdImportReport>,
    /// Status line for the future panel (plain English document data —
    /// see the module docs).
    pub status: String,
}

// Tuned default (a real set name, not empty): manual impl, not derivable.
impl Default for PsdImportState {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            skipped: Vec::new(),
            applied: 0,
            session_set_name: "TextureSet".to_string(),
            last_report: None,
            status: String::new(),
        }
    }
}

impl PsdImportState {
    /// Retargets the session set name (mesh load / project open).
    pub fn retarget_set(&mut self, name: &str) {
        self.session_set_name = name.to_string();
    }

    /// Records one file name's route (shared by the dir scan, the drop
    /// handler, and name-only fixtures).
    pub fn ingest_name(&mut self, file_name: &str, path: Option<PathBuf>) {
        if is_psd_file(Path::new(file_name)) {
            self.pending.push(PendingPsd {
                file: file_name.to_string(),
                path,
                set_name: set_name_from_file(file_name),
            });
        } else {
            self.skipped.push(UnmatchedPsd {
                file: file_name.to_string(),
                reason: PsdSkippedReason::NotPsd,
            });
        }
    }

    /// Clears the last scan and ingests every entry directly under `dir`
    /// (non-recursive v1, same boundary as the mesh-map scan). Missing /
    /// unreadable dirs report through `status` and leave the previous
    /// scan in place.
    pub fn scan_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) => {
                self.status = format!("psd scan failed: {err}");
                return;
            }
        };
        let mut names: Vec<(String, PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                names.push((name.to_string(), path));
            }
        }
        names.sort_by(|a, b| a.0.cmp(&b.0));
        self.pending.clear();
        self.skipped.clear();
        for (name, path) in &names {
            self.ingest_name(name, Some(path.clone()));
        }
        self.status = format!(
            "psd scan: {} matched, {} skipped",
            self.pending.len(),
            self.skipped.len()
        );
    }

    /// The file-drop handler shape: dropped files/folders ingest by name
    /// (folders expand one level like [`Self::scan_dir`]). Appends to the
    /// current lists — a drop never clears a scan under review.
    pub fn ingest_paths(&mut self, paths: &[PathBuf]) {
        for path in paths {
            if path.is_dir() {
                let mut children: Vec<PathBuf> = std::fs::read_dir(path)
                    .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
                    .unwrap_or_default();
                children.sort();
                for child in &children {
                    if child.is_dir() {
                        continue;
                    }
                    let name = child
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                        .to_string();
                    if name.is_empty() {
                        continue;
                    }
                    self.ingest_name(&name, Some(child.clone()));
                }
            } else {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                self.ingest_name(&name, Some(path.clone()));
            }
        }
        self.status = format!(
            "psd scan: {} matched, {} skipped",
            self.pending.len(),
            self.skipped.len()
        );
    }

    /// egui file drops this frame: every non-mesh dropped file routes
    /// through the scan (the mesh-map `poll_drops` mirror; `.psd`
    /// files match here, everything else lands in `skipped` as NotPsd).
    pub fn poll_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            self.ingest_paths(&dropped);
        }
    }

    /// Flattens one pending file's bytes (loaded by `read`) into its
    /// texture-set truth, recording the report + status. The GPU/Document
    /// landing rides the menu wiring; this is the pure half.
    pub fn import_one(
        &mut self,
        pending: &PendingPsd,
        read: impl Fn(&PendingPsd) -> Option<Vec<u8>>,
    ) -> Result<PsdImport, PsdImportError> {
        let Some(bytes) = read(pending) else {
            return Err(PsdImportError::Read {
                detail: format!("could not read file bytes for {}", pending.file),
            });
        };
        let import = import_psd_bytes(&bytes, &pending.file)?;
        self.applied += 1;
        self.status = format!("imported {} as set {}", pending.file, import.set_name);
        self.last_report = Some(import.report.clone());
        Ok(import)
    }
}

/// The Assets panel's PSD section (the mesh-map `show` mirror): the
/// drop hint, the routed table, the Import button, and the import
/// report. v1's apply routes the flatten result into the import
/// state's report (the texture-set/paint landing is the named
/// follow-up this wave's report records); the report rows always show.
pub fn show_section(ui: &mut egui::Ui, state: &mut PsdImportState) {
    use crate::i18n::tr;
    ui.heading(tr("psd-import.title"));
    ui.label(tr("psd-import.hint"));
    ui.add_space(4.0);
    if state.pending.is_empty() && state.skipped.is_empty() {
        ui.label(tr("psd-import.no-files"));
        return;
    }
    ui.label(crate::i18n::tr_args(
        "psd-import.scanned",
        [
            ("matched", fluent_count(state.pending.len())),
            ("unmatched", fluent_count(state.skipped.len())),
        ],
    ));
    for pending in &state.pending {
        let line = crate::i18n::tr_args(
            "psd-import.row-matched",
            [
                ("file", fluent_str(&pending.file)),
                ("set", fluent_str(&pending.set_name)),
            ],
        );
        ui.label(line);
    }
    for skipped in &state.skipped {
        let reason =
            crate::i18n::tr(format!("psd-import.reason-{}", skipped.reason.key()).as_str());
        let line = crate::i18n::tr_args(
            "psd-import.row-unmatched",
            [
                ("file", fluent_str(&skipped.file)),
                ("reason", fluent_str(&reason)),
            ],
        );
        ui.label(line);
    }
    if ui.button(tr("psd-import.apply")).clicked() {
        let pending: Vec<PendingPsd> = state.pending.clone();
        for file in &pending {
            if let Err(err) = state.import_one(file, read_pending_psd_from_disk) {
                state.status = format!("psd import failed: {err}");
            }
        }
    }
    if !state.status.is_empty() {
        ui.label(&state.status);
    }
    if let Some(report) = &state.last_report {
        ui.add_space(4.0);
        ui.label(tr("psd-import.report-title"));
        for line in report.summary() {
            ui.label(line);
        }
    }
}

/// Fluent numeric value helper (the i18n layer's FluentValue shape).
fn fluent_count(n: usize) -> fluent::FluentValue<'static> {
    fluent::FluentValue::from(n.to_string())
}

/// Fluent string value helper.
fn fluent_str(s: &str) -> fluent::FluentValue<'static> {
    fluent::FluentValue::from(s.to_string())
}

/// Reads a pending file's bytes from disk (the future panel's `read`).
pub fn read_pending_psd_from_disk(pending: &PendingPsd) -> Option<Vec<u8>> {
    let path = pending.path.as_ref()?;
    // Size first: `fs::read` of a multi-GB file would allocate all of it
    // before the reader's own caps ever run.
    if std::fs::metadata(path).ok()?.len() > MAX_PSD_FILE_BYTES {
        log::warn!(
            "psd import: {} is over {MAX_PSD_FILE_BYTES} bytes; skipped",
            path.display()
        );
        return None;
    }
    std::fs::read(path).ok()
}

#[cfg(test)]
mod tests {
    use ag_psd::psd::PixelData;

    use super::*;
    use ag_psd::psd::{AdjustmentLayer, InvertAdjustment, LayerAdditionalInfo, LayerMaskData};

    fn rgba8(pixels: &[[u8; 4]]) -> Vec<u8> {
        pixels.concat()
    }

    fn pixel_layer(
        name: &str,
        left: f64,
        top: f64,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> PsdLayer {
        PsdLayer {
            additional_info: LayerAdditionalInfo {
                name: Some(name.to_string()),
                ..Default::default()
            },
            left: Some(left),
            top: Some(top),
            right: Some(left + f64::from(width)),
            bottom: Some(top + f64::from(height)),
            canvas: Some(PixelData {
                width,
                height,
                data: pixels,
            }),
            ..Default::default()
        }
    }

    fn solid_layer(name: &str, width: u32, height: u32, pixel: [u8; 4]) -> PsdLayer {
        let count = width as usize * height as usize;
        pixel_layer(name, 0.0, 0.0, width, height, rgba8(&vec![pixel; count]))
    }

    fn doc(children: Vec<PsdLayer>, width: f64, height: f64) -> Psd {
        Psd {
            width,
            height,
            color_mode: Some(ColorMode::Rgb),
            children: Some(children),
            ..Default::default()
        }
    }

    fn px(pixels: &[u8]) -> Vec<[u8; 4]> {
        pixels
            .chunks_exact(4)
            .map(|px| [px[0], px[1], px[2], px[3]])
            .collect()
    }

    #[test]
    fn file_order_composites_bottom_to_top() {
        // Bottom blue, top red, both opaque: the top record wins every
        // texel. This pins the implementation's z-order convention (see
        // the module docs — file order treated as bottom-to-top).
        let psd = doc(
            vec![
                solid_layer("bottom", 2, 2, [10, 20, 30, 255]),
                solid_layer("top", 2, 2, [200, 100, 50, 255]),
            ],
            2.0,
            2.0,
        );
        let import = flatten_psd(&psd, "Sword.psd").unwrap();
        assert_eq!(import.set_name, "Sword");
        assert_eq!((import.width, import.height), (2, 2));
        assert_eq!(import.report.path, FlattenPath::FallbackRaster);
        assert_eq!(import.report.layers_seen, 2);
        assert_eq!(import.report.pixel_layers, 2);
        for pixel in px(&import.pixels) {
            assert_eq!(pixel, [200, 100, 50, 255]);
        }
    }

    #[test]
    fn multiply_and_screen_hit_their_identities() {
        // White multiply and black screen are both no-ops: the exact
        // umber-core math must round-trip the destination bytes.
        for (mode, src) in [
            (PsdBlendMode::Multiply, [255, 255, 255, 255]),
            (PsdBlendMode::Screen, [0, 0, 0, 255]),
        ] {
            let mut top = solid_layer("top", 2, 2, src);
            top.blend_mode = Some(mode);
            let psd = doc(
                vec![solid_layer("bottom", 2, 2, [100, 150, 200, 255]), top],
                2.0,
                2.0,
            );
            let import = flatten_psd(&psd, "m.psd").unwrap();
            assert!(import.report.substitutions.is_empty());
            for pixel in px(&import.pixels) {
                assert_eq!(pixel, [100, 150, 200, 255]);
            }
        }
    }

    #[test]
    fn half_opacity_averages_exactly() {
        // Opacity 0.5 is exact in f32: (200+100)/2=150, (100+100)/2=100,
        // (50+100)/2=75 — no rounding-boundary risk by construction.
        let mut top = solid_layer("top", 2, 2, [200, 100, 50, 255]);
        top.opacity = Some(0.5);
        let psd = doc(
            vec![solid_layer("bottom", 2, 2, [100, 100, 100, 255]), top],
            2.0,
            2.0,
        );
        let import = flatten_psd(&psd, "o.psd").unwrap();
        for pixel in px(&import.pixels) {
            assert_eq!(pixel, [150, 100, 75, 255]);
        }
    }

    #[test]
    fn group_opacity_multiplies_the_whole_group() {
        // Folder at 0.5 holding opaque red over opaque blue: coverage is
        // exactly 0.5, so both tergets land on 127.5 -> round-away -> 128
        // (deterministic: the midpoint is exact in f32).
        let child = solid_layer("child", 2, 2, [255, 0, 0, 255]);
        let group = PsdLayer {
            additional_info: LayerAdditionalInfo {
                name: Some("group".to_string()),
                ..Default::default()
            },
            opacity: Some(0.5),
            children: Some(vec![child]),
            ..Default::default()
        };
        let psd = doc(
            vec![solid_layer("bottom", 2, 2, [0, 0, 255, 255]), group],
            2.0,
            2.0,
        );
        let import = flatten_psd(&psd, "g.psd").unwrap();
        assert_eq!(import.report.groups_seen, 1);
        assert!(
            import
                .report
                .notes
                .iter()
                .any(|note| note.contains("group-wide multiply")),
            "{:?}",
            import.report.notes
        );
        for pixel in px(&import.pixels) {
            assert_eq!(pixel, [128, 0, 128, 255]);
        }
    }

    #[test]
    fn simple_grayscale_mask_scales_coverage() {
        // Mask 128 over white-on-black: coverage 128/255, so white lands
        // at exactly 128 (true value; f32 error ~1e-5, far from any
        // rounding boundary).
        let mut top = solid_layer("top", 2, 2, [255, 255, 255, 255]);
        top.additional_info.mask = Some(LayerMaskData {
            canvas: Some(PixelData {
                width: 2,
                height: 2,
                data: rgba8(&[[128, 128, 128, 255]; 4]),
            }),
            ..Default::default()
        });
        let psd = doc(
            vec![solid_layer("bottom", 2, 2, [0, 0, 0, 255]), top],
            2.0,
            2.0,
        );
        let import = flatten_psd(&psd, "mask.psd").unwrap();
        assert_eq!(import.report.masks_applied.len(), 1);
        assert!(import.report.masks_skipped.is_empty());
        for pixel in px(&import.pixels) {
            assert_eq!(pixel, [128, 128, 128, 255]);
        }
    }

    #[test]
    fn full_document_mask_applies_to_cropped_layers() {
        // The real-file shape: a 2x2 mask at doc origin, a 1x1 layer at
        // (1,1). Mask texel (1,1) is 0 (cut), (0,0) is 255 — but the layer
        // only covers (1,1), so it vanishes entirely over the base.
        let mut top = pixel_layer("top", 1.0, 1.0, 1, 1, vec![255, 0, 0, 255]);
        top.additional_info.mask = Some(LayerMaskData {
            canvas: Some(PixelData {
                width: 2,
                height: 2,
                data: rgba8(&[
                    [255, 255, 255, 255],
                    [255, 255, 255, 255],
                    [255, 255, 255, 255],
                    [0, 0, 0, 255],
                ]),
            }),
            ..Default::default()
        });
        let psd = doc(
            vec![solid_layer("bottom", 2, 2, [0, 0, 255, 255]), top],
            2.0,
            2.0,
        );
        let import = flatten_psd(&psd, "crop.psd").unwrap();
        assert_eq!(import.report.masks_applied.len(), 1);
        for pixel in px(&import.pixels) {
            assert_eq!(pixel, [0, 0, 255, 255]);
        }
    }

    #[test]
    fn unimplemented_blend_flattens_as_normal_and_reports() {
        let mut top = solid_layer("fx", 2, 2, [200, 100, 50, 255]);
        top.blend_mode = Some(PsdBlendMode::Overlay);
        let psd = doc(
            vec![solid_layer("bottom", 2, 2, [10, 20, 30, 255]), top],
            2.0,
            2.0,
        );
        let import = flatten_psd(&psd, "fx.psd").unwrap();
        assert_eq!(import.report.substitutions.len(), 1);
        assert!(import.report.substitutions[0].contains("Overlay"));
        assert!(import.report.substitutions[0].contains("normal"));
        // Flattened as normal: opaque source wins verbatim.
        for pixel in px(&import.pixels) {
            assert_eq!(pixel, [200, 100, 50, 255]);
        }
    }

    #[test]
    fn hidden_empty_and_clipped_layers_report_honestly() {
        let mut hidden = solid_layer("hidden", 2, 2, [9, 9, 9, 255]);
        hidden.hidden = Some(true);
        let empty = PsdLayer {
            additional_info: LayerAdditionalInfo {
                name: Some("curves".to_string()),
                adjustment: Some(AdjustmentLayer::Invert(InvertAdjustment)),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut clipped = solid_layer("clip", 2, 2, [1, 2, 3, 255]);
        clipped.clipping = Some(true);
        let psd = doc(
            vec![
                solid_layer("base", 2, 2, [10, 20, 30, 255]),
                hidden,
                empty,
                clipped,
            ],
            2.0,
            2.0,
        );
        let import = flatten_psd(&psd, "mix.psd").unwrap();
        // base + clipped composite; hidden + adjustment skipped.
        assert_eq!(import.report.pixel_layers, 2);
        assert!(
            import
                .report
                .notes
                .iter()
                .any(|note| note.contains("hidden")),
            "{:?}",
            import.report.notes
        );
        assert!(
            import
                .report
                .notes
                .iter()
                .any(|note| note.contains("adjustment")),
            "{:?}",
            import.report.notes
        );
        assert!(
            import
                .report
                .substitutions
                .iter()
                .any(|line| line.contains("clipping")),
            "{:?}",
            import.report.substitutions
        );
        // Clipped layer flattened unclipped on top: opaque wins.
        for pixel in px(&import.pixels) {
            assert_eq!(pixel, [1, 2, 3, 255]);
        }
    }

    #[test]
    fn layer_placement_and_canvas_clipping() {
        // 1x1 red at (1,1) over a 2x2 blue base: only the corner turns.
        let top = pixel_layer("dot", 1.0, 1.0, 1, 1, vec![255, 0, 0, 255]);
        let psd = doc(
            vec![solid_layer("base", 2, 2, [0, 0, 255, 255]), top],
            2.0,
            2.0,
        );
        let import = flatten_psd(&psd, "place.psd").unwrap();
        assert_eq!(
            px(&import.pixels),
            vec![
                [0, 0, 255, 255],
                [0, 0, 255, 255],
                [0, 0, 255, 255],
                [255, 0, 0, 255],
            ]
        );
        // Hanging off the canvas clips loudly, never wraps.
        let off = pixel_layer("off", 2.0, 0.0, 1, 1, vec![0, 255, 0, 255]);
        let psd = doc(
            vec![solid_layer("base", 2, 2, [0, 0, 255, 255]), off],
            2.0,
            2.0,
        );
        let import = flatten_psd(&psd, "off.psd").unwrap();
        assert!(
            import
                .report
                .notes
                .iter()
                .any(|note| note.contains("clipped")),
            "{:?}",
            import.report.notes
        );
        for pixel in px(&import.pixels) {
            assert_eq!(pixel, [0, 0, 255, 255]);
        }
    }

    #[test]
    fn empty_document_is_an_error_not_silence() {
        let psd = doc(vec![], 4.0, 4.0);
        let err = flatten_psd(&psd, "empty.psd").unwrap_err();
        assert!(matches!(err, PsdImportError::NoPixelTruth { .. }));
    }

    #[test]
    fn corrupt_composite_falls_back_to_layers() {
        // A composite whose length disagrees with its dims is malformed —
        // the import must use the layers and SAY so, not trust the bytes.
        let psd = Psd {
            width: 2.0,
            height: 2.0,
            color_mode: Some(ColorMode::Rgb),
            children: Some(vec![solid_layer("only", 2, 2, [7, 8, 9, 255])]),
            canvas: Some(PixelData {
                width: 2,
                height: 2,
                data: vec![0u8; 3],
            }),
            ..Default::default()
        };
        let import = flatten_psd(&psd, "corrupt.psd").unwrap();
        assert_eq!(import.report.path, FlattenPath::FallbackRaster);
        assert!(
            import
                .report
                .notes
                .iter()
                .any(|note| note.contains("malformed")),
            "{:?}",
            import.report.notes
        );
        for pixel in px(&import.pixels) {
            assert_eq!(pixel, [7, 8, 9, 255]);
        }
    }

    #[test]
    fn oversized_dimensions_are_refused() {
        let psd = doc(vec![], 9000.0, 9000.0);
        assert!(matches!(
            flatten_psd(&psd, "huge.psd").unwrap_err(),
            PsdImportError::InvalidDimensions { .. }
        ));
    }

    #[test]
    fn export_round_trip_imports_the_composite_verbatim() {
        // The kill's own writer path (umber-export's ag-psd writer, the
        // `40909ea` export): write a layered file through OUR writer,
        // import it back, and the composite must match what the writer
        // encoded (the first map — baseColor here) byte for byte.
        use umber_export::driver::MapSet;
        use umber_export::presets::MapKind;

        let size = 4u32;
        let mut map = vec![0u8; (size * size * 4) as usize];
        for pixel in map.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[200, 200, 200, 255]);
        }
        let mut rough = vec![0u8; (size * size * 4) as usize];
        for pixel in rough.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[64, 64, 64, 255]);
        }
        let mut set = MapSet::new(size);
        set.set(MapKind::BaseColor, map.clone());
        set.set(MapKind::Roughness, rough);

        // Opaque fixtures only: the writer premultiplies partial alpha
        // toward white (writer.rs' "weird white matte"), so only opaque
        // composites round-trip byte-identically — correctly, since map
        // sets are opaque (same rule as the export's own test).
        // A per-TEST dir: the sibling test at the end of this module
        // `remove_dir_all`s its own dir, and libtest runs them in parallel —
        // a shared `umber-psd-test-<pid>` raced (the create here, the
        // sibling's delete, then this write → ENOENT).
        let dir =
            std::env::temp_dir().join(format!("umber-psd-test-roundtrip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.psd");
        umber_export::psd::write_psd_maps(&set, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let import = import_psd_bytes(&bytes, "roundtrip.psd").unwrap();
        assert_eq!(import.report.path, FlattenPath::Composite);
        assert_eq!(import.set_name, "roundtrip");
        assert_eq!((import.width, import.height), (size, size));
        assert_eq!(import.pixels, map);
        assert_eq!(import.report.layers_seen, 2);
    }

    #[test]
    fn truncated_and_garbage_inputs_are_errors() {
        assert!(import_psd_bytes(&[], "empty.psd").is_err());
        assert!(import_psd_bytes(&[0u8; 10], "short.psd").is_err());
        assert!(import_psd_bytes(b"definitely not a photoshop file", "x.psd").is_err());
        // A valid header naming 3 channels of a 4x4 RGB8 file, then a
        // layer section claiming 1000 records with no bytes behind them:
        // the reader runs out of buffer — an error, never a panic.
        let mut bytes = vec![0u8; 26];
        bytes[0..4].copy_from_slice(b"8BPS");
        bytes[4..6].copy_from_slice(&1u16.to_be_bytes());
        bytes[10..12].copy_from_slice(&3u16.to_be_bytes());
        bytes[12..16].copy_from_slice(&4u32.to_be_bytes());
        bytes[16..20].copy_from_slice(&4u32.to_be_bytes());
        bytes[20..22].copy_from_slice(&8u16.to_be_bytes());
        bytes[22..24].copy_from_slice(&3u16.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.extend_from_slice(&2u32.to_be_bytes());
        bytes.extend_from_slice(&1000i16.to_be_bytes());
        assert!(import_psd_bytes(&bytes, "bogus.psd").is_err());
    }

    #[test]
    fn header_sniff_names_modes_and_rejects_garbage() {
        fn header(mode: u16) -> Vec<u8> {
            let mut bytes = vec![0u8; 26];
            bytes[0..4].copy_from_slice(b"8BPS");
            bytes[4..6].copy_from_slice(&1u16.to_be_bytes());
            bytes[24..26].copy_from_slice(&mode.to_be_bytes());
            bytes
        }
        assert_eq!(sniff_color_mode(&header(3)).as_deref(), Some("Rgb"));
        assert_eq!(sniff_color_mode(&header(4)).as_deref(), Some("Cmyk"));
        assert_eq!(sniff_color_mode(&header(9)).as_deref(), Some("Lab"));
        assert_eq!(sniff_color_mode(&header(1)).as_deref(), Some("Grayscale"));
        assert_eq!(sniff_color_mode(&[]), None);
        assert_eq!(sniff_color_mode(&[0u8; 26]), None);
        assert_eq!(
            sniff_color_mode(&header(99)).as_deref(),
            Some("unknown(99)")
        );
        // The mapper: CMYK-declaring bytes route to the named rejection,
        // everything else stays a verbatim read error.
        let err = map_read_error("Color mode not supported: Cmyk", &header(4));
        assert!(matches!(err, PsdImportError::UnsupportedColorMode { .. }));
        let err = map_read_error("UnexpectedEndOfBuffer", &header(3));
        assert!(matches!(err, PsdImportError::Read { .. }));
    }

    #[test]
    fn naming_and_routing_match_the_drop_shape() {
        assert_eq!(set_name_from_file("Sword.psd"), "Sword");
        assert_eq!(set_name_from_file("/tmp/my_set_v2.PSD"), "my_set_v2");
        assert_eq!(set_name_from_file("archive.tar.psb"), "archive.tar");
        assert!(is_psd_file(Path::new("a.psd")));
        assert!(is_psd_file(Path::new("b.PSB")));
        assert!(!is_psd_file(Path::new("c.png")));
        assert!(!is_psd_file(Path::new("nodivider")));
        assert_eq!(PsdSkippedReason::NotPsd.key(), "psd-import.reason-not-psd");
    }

    #[test]
    fn scan_routes_psd_and_reports_the_rest() {
        let dir = std::env::temp_dir().join(format!("umber-psd-test-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Sword.psd"), b"fake").unwrap();
        std::fs::write(dir.join("notes.txt"), b"nope").unwrap();
        let mut state = PsdImportState::default();
        assert_eq!(state.session_set_name, "TextureSet");
        state.retarget_set("Sword");
        state.scan_dir(&dir);
        assert_eq!(state.pending.len(), 1);
        assert_eq!(state.pending[0].set_name, "Sword");
        assert_eq!(state.skipped.len(), 1);
        assert_eq!(state.skipped[0].reason, PsdSkippedReason::NotPsd);
        // A drop appends without clearing the scan under review.
        state.ingest_paths(&[dir.join("Shield.psb")]);
        assert_eq!(state.pending.len(), 2);
        assert_eq!(state.pending[1].set_name, "Shield");
        // Unreadable bytes stay an error through the state half too.
        let pending = state.pending[0].clone();
        let err = state
            .import_one(&pending, |_| Some(b"bogus".to_vec()))
            .unwrap_err();
        assert!(matches!(err, PsdImportError::Read { .. }));
        assert_eq!(state.applied, 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn report_summary_is_deterministic_and_names_v2() {
        let psd = doc(vec![solid_layer("only", 1, 1, [1, 2, 3, 255])], 1.0, 1.0);
        let import = flatten_psd(&psd, "One.psd").unwrap();
        let summary = import.report.summary();
        assert!(summary[0].contains("One.psd"));
        assert!(summary[0].contains("One"));
        assert!(summary.iter().any(|line| line.contains("fallback")));
        assert!(
            summary
                .last()
                .is_some_and(|line| line.contains("v2 (deferred)")),
            "{summary:?}"
        );
    }

    #[test]
    fn publish_linearizes_colour_and_premultiplies() {
        // The C2-32 pinned decision: sRGB 188 -> linear 128, alpha
        // untouched through the curve, THEN premultiplied (RGB scaled
        // by alpha/255).
        let flat = PsdImport {
            set_name: "Set".into(),
            width: 1,
            height: 1,
            pixels: vec![188, 188, 188, 128],
            report: PsdImportReport::default(),
        };
        let out = publish_psd_pixels(&flat);
        // 188 sRGB -> 128 linear; 128 * (128/255) = 64.25 -> 64 by
        // f32_to_u8's round; alpha rides at 128.
        assert_eq!(out, vec![64, 64, 64, 128]);
        // The input is not consumed:
        assert_eq!(flat.pixels, vec![188, 188, 188, 128]);
    }

    #[test]
    fn publish_leaves_opaque_black_and_white_exact() {
        // sRGB 0 -> 0 and sRGB 255 -> 255 under the curve; alpha 255
        // premultiplies to identity. The extremes are the curve's
        // pinned endpoints (shared with curvature_png_to_linear).
        let flat = PsdImport {
            set_name: "Set".into(),
            width: 2,
            height: 1,
            pixels: vec![0, 0, 0, 255, 255, 255, 255, 255],
            report: PsdImportReport::default(),
        };
        let out = publish_psd_pixels(&flat);
        assert_eq!(out, vec![0, 0, 0, 255, 255, 255, 255, 255]);
    }
}
