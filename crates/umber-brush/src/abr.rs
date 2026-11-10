//! Clean-room reader for Adobe Photoshop `.abr` brush files (v6/v7/v9/v10):
//! the `samp` tip bitmaps, since row 282 of the wave-abr design brief
//! (`docs/loop/briefs/2026-11-08-wave-abr-desc-design.md`) the `desc`
//! brush descriptors, and since wave 3.1a
//! (`docs/loop/briefs/2026-11-09-abr-patt-reader-kill-brief.md`) the `patt`
//! brush textures.
//!
//! What a `samp` record IS (verified against real files, 2026-11-07):
//! a **sampled tip image**, not a brush. Real v6/v7/v9/v10 files carry
//! the tip bitmaps in `samp` (each with a UUID-ish id string, an
//! absolute position and a size) and the BRUSHES themselves — their
//! names, sizes, hardness, spacing, dynamics — in the descriptor
//! (`desc`) section. [`read_abr`] returns both: the samples
//! ([`AbrSample`]) and the brushes ([`AbrBrush`]), and [`match_sample`]
//! links a `sampledBrush` to its tip through the descriptor's
//! `sampledData` id.
//!
//! Provenance: the section walk, the PackBits/RLE decoder and the brush
//! mapping are a clean-room port of the *logic* in the vendored `ag-psd`
//! reference (`docs/claw-artifacts/ag-psd/src-tree/src/abr.rs`: `read_abr`,
//! `read_data_rle`, `parse_brush`, `parse_brush_shape`, `parse_dynamics`);
//! the descriptor core lives in `abr_desc.rs` (ported from the reference's
//! `descriptor.rs`). No reference code, comments or prose were copied.
//! That reference is this module's format authority: where it disagrees
//! with a prose description, the reference wins. Two consequences are
//! worth stating up front because they are easy to get wrong:
//!
//! * every multi-byte field is **big-endian** (the reference reads all of
//!   them with `from_be_bytes`, the PSD-family convention);
//! * the byte after the 16-bit bit depth is a **one-byte compression
//!   code** (0 = raw, 1 = PackBits/RLE), not a 16-bit row count.
//!
//! Supported: version 6, 7, 9 and 10 (minor version 1 or 2) — the
//! versions the reference accepts and the ones real files use — the
//! `8BIM` section walk, `samp` sample records, 8-bit raw and RLE alpha,
//! 16-bit raw alpha (the high byte is kept), and (row 282, LANDED) the
//! `desc` section: a version-16 descriptor whose `Brsh` list holds one
//! brush descriptor per brush. Each brush's outer class id names its tool
//! (`MixB` mixer, `SmTl` smudge, anything else — `_` in practice — the
//! plain brush), its `Brsh` key holds the tip-shape descriptor, and the
//! `computedBrush` / `sampledBrush` shape classes are mapped to
//! [`AbrBrush`]; every other shape class (`dBrush`, `dTips`, ...) is
//! skipped and counted in [`AbrFile::skipped_shape_classes`]. A `phry`
//! section is parsed as a descriptor and ignored.
//!
//! Since wave 3.1a a `patt` section is decoded into
//! [`AbrFile::patterns`] ([`AbrPattern`], RGBA8, alpha fixed at 255) and a
//! brush whose descriptor sets `useTexture` and carries a `Txtr` descriptor
//! gets [`AbrBrush::texture`]; [`match_pattern`] resolves the brush's
//! `Txtr.Idnt` against the decoded patterns. A pattern section that decodes
//! to nothing is not an error (see the soft-decoding note below). Unknown
//! sections are still skipped by their declared length.
//!
//! Deliberate non-goals (the v1 boundary):
//!
//! * sampling a brush texture in the paint engine (kill 3.1c): carrying the
//!   pattern does not change a painted pixel;
//! * naming the stored `BlnM.<code>` blend-mode string;
//! * v8+ files, which are descriptor-only and rejected as unsupported;
//! * unknown descriptor OSTypes — a named strict error
//!   ([`AbrError::UnsupportedDescriptorType`]); extending the table is a
//!   later kill if a real file needs it;
//! * dual brush, scattering, color dynamics, transfer, and every dynamics
//!   channel except size (`szVr`): the descriptor reader parses those keys
//!   but nothing maps them (the texture keys ARE mapped, see
//!   [`AbrBrush::texture`]);
//! * mapping an [`AbrBrush`] onto a `BrushPreset` (row 283);
//! * 16-bit RLE (rejected with a named error);
//! * writing `.abr` — never (the format is read-only for umber).
//!
//! Safety caps (hostile input): at most [`MAX_SAMPLES`] samples per file,
//! at most [`MAX_STAMP_DIMENSION`] x [`MAX_STAMP_DIMENSION`] pixels per
//! stamp, at most [`MAX_PATTERNS`] patterns per file with
//! [`MAX_PATTERN_DIMENSION`] pixels per side, [`MAX_TOTAL_ALPHA_BYTES`] of
//! decoded stamp AND pattern bytes per file (one shared budget, so a file
//! cannot spend it twice), and at most [`MAX_DESC_SECTION_BYTES`] per
//! declared descriptor section (plus the descriptor reader's own item /
//! list / text / raw / depth / reference caps, `abr_desc.rs`). Every cap is
//! checked *before* the allocation it guards. The stamp caps are *strict*
//! (a breach refuses the file); the pattern caps are *soft* (a breach skips
//! the record — see below).
//!
//! Judgment calls:
//!
//! * **Pattern decoding is soft, deliberately unlike `desc`.** A `patt`
//!   section has been accepted by skipping since row 268, so no pattern
//!   problem may turn a file that imports today into a refusal. An
//!   undecodable record is skipped by its declared length and counted in
//!   [`AbrFile::skipped_patterns`]; a record whose declared length runs
//!   past the section ends pattern parsing for that section and is counted
//!   once. [`read_abr`] therefore never fails because of `patt` content —
//!   not even a hostile one. The per-record rules live in `abr_patt.rs`.
//! * The per-brush parameter block between the name and the bounds is 10
//!   bytes at minor version 1 and 264 bytes at minor version 2. The
//!   reference implementation SKIPS the whole block without naming its
//!   fields, and the brush's real settings live in the descriptor, so the
//!   block is skipped, never interpreted.
//! * [`read_samples`] keeps its row-268 behavior EXACTLY: it runs the
//!   same walk in a samples-only mode that skips `desc`/`phry`/`patt` by
//!   length instead of parsing them. Descriptor parsing is strict (an
//!   unknown OSType is an error), so routing the samples-only entry point
//!   through it would turn a working tip import into a refusal on any real
//!   file whose descriptor holds an OSType outside the table. [`read_abr`]
//!   is the strict full read (strict about everything except `patt`).
//! * [`AbrBrush::texture`] is `Some` only when `useTexture` is true AND
//!   `Txtr` is a descriptor: a `Txtr` without the gate is dead data (the
//!   reference's rule), and a texture with no pattern record still resolves
//!   to a texture whose id matches nothing ([`match_pattern`] returns
//!   `None`) — the import layer, not the reader, decides what a dangling
//!   link means.
//! * The texture's percent fields keep [`parse_percent`]'s "missing →
//!   1.0" default and its wrong-units error; the blend mode is the stored
//!   `as_text` value (`BlnM.<code>`) undecoded — naming the code is
//!   3.1b/3.1c's call.
//! * A brush's name is its outer `Nm  ` key, falling back to the shape's
//!   `Nm  ` (sampled shapes carry one too); missing both → empty.
//! * `hardness` reads the shape's `Hrdn` for both shape classes (missing
//!   → 1.0, the percent default); `spacing_on` / `spacing` come from the
//!   shape's `Intr` / `Spcn`.
//! * [`AbrBrush::dynamics`] is the SIZE dynamics (`szVr`), present only
//!   when the brush's `useTipDynamics` is true (the reference's gate).
//!   `bVTy` / `fStp` stay the stored integers: naming the control source
//!   and mapping it onto Umber's inputs is row 283's call.
//! * A brush descriptor without a `Brsh` shape descriptor is malformed
//!   (the reference refuses it too); a `Brsh` list entry that is not a
//!   descriptor is ignored (the reference ignores it).
//! * The raw descriptor tree types (`Descriptor`, `DescValue`, ...) are
//!   re-exported so a later kill (row 283, a CLI inspector) can read the
//!   keys this module does not map.
//! * PackBits header byte 128 is a no-op: the reference ignores it. It
//!   does **not** mean "repeat the next byte three times".
//! * A row whose RLE stream decodes fewer than `width` bytes leaves the
//!   remainder of that row zero (the reference's behavior). A stream that
//!   claims bytes past the end of its row is a malformed error.
//! * Sample ids are decoded byte-for-byte as Latin-1 (each byte becomes
//!   the code point of the same value), matching the reference's
//!   `read_short_string`; id decoding can never fail.
//! * A file with no `samp` / `desc` section parses to empty lists rather
//!   than an error: the section walk is length-delimited, so the absence
//!   of stamps is not corruption.

use thiserror::Error;

use crate::abr_desc::{
    as_bool, as_descriptor, as_number, as_text, dget, parse_angle, parse_percent,
    parse_units_to_number, read_version_and_descriptor,
};

pub use crate::abr_desc::{DescValue, Descriptor, ObjectArrayItem, RefItem};
pub use crate::abr_patt::{AbrPattern, MAX_PATTERNS, MAX_PATTERN_DIMENSION};

use crate::abr_patt::parse_patterns;

/// Maximum declared byte length of one `desc` / `phry` section.
pub const MAX_DESC_SECTION_BYTES: usize = 64 * 1024 * 1024;

/// Maximum number of samples accepted from one file.
pub const MAX_SAMPLES: usize = 4096;

/// Maximum accepted stamp width and height, in pixels.
pub const MAX_STAMP_DIMENSION: u32 = 4096;

/// The maximum TOTAL decoded plane data one file may produce, in bytes.
///
/// The per-stamp cap alone still allows 4096 stamps of 16 MiB each — 64
/// GiB of decoded planes out of a small hostile file — so the sum is
/// capped too. Stamps and patterns draw on the SAME budget: the reader
/// keeps one running total and pattern records are charged as they are
/// decoded (a pattern that would overshoot is skipped, not fatal).
pub const MAX_TOTAL_ALPHA_BYTES: usize = 256 * 1024 * 1024;

/// One parsed **sample** (a tip bitmap) from an `.abr` file's `samp`
/// section.
///
/// A sample is NOT a brush: real files put the tip bitmaps here, keyed
/// by an id string, and the brushes (name, size, hardness, spacing,
/// dynamics) in the descriptor (`desc`) section. A caller matches a
/// brush to its tip through the descriptor's sampled-data id.
#[derive(Debug, Clone)]
pub struct AbrSample {
    /// The sample's id string, decoded byte-for-byte as Latin-1 (real
    /// files use a UUID-ish form).
    pub id: String,
    /// The sample's left edge in the file's coordinate space.
    pub x: i32,
    /// The sample's top edge in the file's coordinate space.
    pub y: i32,
    /// Sample width in pixels.
    pub width: u32,
    /// Sample height in pixels.
    pub height: u32,
    /// 8-bit alpha plane, row-major, exactly `width * height` bytes.
    pub alpha: Vec<u8>,
}

/// Errors [`read_abr`] can return.
///
/// Every failure is a named variant: the parser never panics on hostile
/// input, and the messages are suitable for showing to a user.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AbrError {
    /// The bytes are not an `.abr` file: the version field is absent or is
    /// not a plausible ABR version, or a section is missing the `8BIM`
    /// signature.
    #[error("not an .abr brush file: missing the 8BIM section signature")]
    NotAbr,
    /// A recognized but unsupported ABR version (v1–v5, or the
    /// descriptor-only v8 and later).
    #[error("unsupported .abr version {version}: only v6, v7, v9 and v10 are supported")]
    UnsupportedVersion {
        /// The version field as stored in the file.
        version: i16,
    },
    /// A v6/v7 file whose minor version is neither 1 nor 2.
    #[error("unsupported .abr minor version {minor}: only 1 and 2 are supported")]
    UnsupportedMinorVersion {
        /// The minor-version field as stored in the file.
        minor: i16,
    },
    /// A structurally invalid or truncated file.
    #[error("malformed .abr brush file: {detail}")]
    Malformed {
        /// What failed, in user-facing terms.
        detail: String,
    },
    /// More than [`MAX_SAMPLES`] sample records.
    #[error("too many samples ({count}): the safety cap is {max} per file")]
    TooManyBrushes {
        /// The count that tripped the cap (one past the maximum).
        count: usize,
        /// The cap itself.
        max: usize,
    },
    /// The file's decoded stamp planes exceed [`MAX_TOTAL_ALPHA_BYTES`].
    #[error("too much stamp data ({bytes} bytes): the safety cap is {max} bytes per file")]
    TooMuchData {
        /// The decoded byte total that tripped the cap.
        bytes: usize,
        /// The cap itself.
        max: usize,
    },
    /// A stamp whose declared bounds exceed [`MAX_STAMP_DIMENSION`].
    #[error("brush stamp {width}x{height} exceeds the {max}x{max} pixel safety cap")]
    StampTooLarge {
        /// Declared stamp width in pixels.
        width: u32,
        /// Declared stamp height in pixels.
        height: u32,
        /// The cap itself, in pixels per side.
        max: u32,
    },
    /// A bit depth other than 8, or 16-bit RLE data.
    #[error("unsupported brush bit depth {depth}: only 8-bit and 16-bit raw stamps are supported")]
    UnsupportedBitDepth {
        /// The bit depth as stored in the file.
        depth: i16,
    },
    /// A compression code other than 0 (raw) or 1 (RLE), or 16-bit RLE.
    #[error(
        "unsupported brush compression {compression}: 0 (raw) and 1 (RLE) are supported for 8-bit stamps, 0 for 16-bit"
    )]
    UnsupportedCompression {
        /// The compression code as stored in the file.
        compression: u8,
    },
    /// A descriptor whose version field is not 16.
    #[error("unsupported .abr descriptor version {0}: only version 16 is supported")]
    UnsupportedDescriptorVersion(u32),
    /// A descriptor value (or reference item) whose four-byte type
    /// signature this reader does not decode.
    #[error("unsupported .abr descriptor value type {0:?}")]
    UnsupportedDescriptorType(String),
    /// A unit code outside the units table, or a value whose units are not
    /// the ones its key requires.
    #[error("invalid .abr descriptor units {0:?}")]
    InvalidDescriptorUnits(String),
    /// A descriptor count, length, nesting depth or section size over its
    /// safety cap (refused before anything is allocated from it).
    #[error("the .abr descriptor's {what} ({count}) exceeds the safety cap of {max}")]
    DescriptorTooLarge {
        /// Which cap tripped (`"text"`, `"list items"`, ...).
        what: &'static str,
        /// The stored value that tripped it.
        count: u64,
        /// The cap itself.
        max: u64,
    },
    /// A truncated or structurally invalid descriptor.
    #[error("malformed .abr descriptor: {detail}")]
    MalformedDescriptor {
        /// What failed, in user-facing terms.
        detail: String,
    },
}

/// The tool a descriptor brush belongs to (the brush descriptor's class
/// id: `MixB`, `SmTl`, anything else).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbrToolClass {
    /// The plain brush tool (class `_`, and the fallback for any other id).
    Brush,
    /// The mixer brush (class `MixB`).
    MixerBrush,
    /// The smudge tool (class `SmTl`).
    Smudge,
}

/// A brush's size dynamics (`szVr`), as stored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbrDynamics {
    /// The control source index (`bVTy`): 0 off, then the file's
    /// fade / pen / tilt / ... order (not named here — row 283).
    pub control_index: i32,
    /// The fade step count (`fStp`).
    pub steps: i32,
    /// Jitter as a fraction (`jitter`, percent / 100).
    pub jitter: f64,
    /// Minimum as a fraction (`Mnm `, percent / 100).
    pub minimum: f64,
}

/// A brush's texture link (`useTexture` + `Txtr`), as stored.
///
/// The pattern itself lives in [`AbrFile::patterns`]: `pattern_id` is the
/// link key (`Txtr.Idnt`), resolved by [`match_pattern`]. Alpha is not a
/// thing here — a pattern is a tiling colour image.
#[derive(Debug, Clone, PartialEq)]
pub struct AbrTexture {
    /// The referenced pattern id (`Txtr.Idnt`, Latin-1-ish text).
    pub pattern_id: String,
    /// The referenced pattern's display name (`Txtr.Nm  `); informational,
    /// the id is the link.
    pub pattern_name: String,
    /// Texture scale as a fraction (`textureScale`, percent / 100;
    /// missing → 1.0).
    pub scale: f64,
    /// Texture depth as a fraction (`textureDepth`, percent / 100;
    /// missing → 1.0).
    pub depth: f64,
    /// Whether the pattern is inverted (`InvT`).
    pub invert: bool,
    /// The stored blend-mode enum string (`BlnM.<code>`, e.g.
    /// `BlnM.CBrn`), undecoded.
    pub blend_mode: String,
    /// Whether the texture applies to each tip (`TxtC`).
    pub each_tip: bool,
}

/// One brush from a `desc` section (`computedBrush` or `sampledBrush`
/// shape).
#[derive(Debug, Clone, PartialEq)]
pub struct AbrBrush {
    /// The tool the brush belongs to.
    pub tool_class: AbrToolClass,
    /// The brush's name (outer `Nm  `, else the shape's `Nm  `).
    pub name: String,
    /// Tip diameter in pixels (`Dmtr`, `#Pxl`).
    pub diameter_px: f64,
    /// Tip angle in degrees (`Angl`, `#Ang`; missing → 0).
    pub angle_deg: f64,
    /// Tip roundness as a fraction (`Rndn`; missing → 1).
    pub roundness: f64,
    /// Whether spacing is enabled (`Intr`).
    pub spacing_on: bool,
    /// Spacing as a fraction of the diameter (`Spcn`; missing → 1).
    pub spacing: f64,
    /// Horizontal flip (`flipX`).
    pub flip_x: bool,
    /// Vertical flip (`flipY`).
    pub flip_y: bool,
    /// Hardness as a fraction (`Hrdn`; missing → 1).
    pub hardness: f64,
    /// The `sampledData` id linking a `sampledBrush` to its
    /// [`AbrSample`]; always `None` for a `computedBrush`.
    pub sampled_data_id: Option<String>,
    /// Size dynamics, when `useTipDynamics` is on.
    pub dynamics: Option<AbrDynamics>,
    /// The brush's texture, when `useTexture` is on and `Txtr` is a
    /// descriptor; the pattern it names is in [`AbrFile::patterns`].
    pub texture: Option<AbrTexture>,
}

/// Everything [`read_abr`] extracts from one file.
#[derive(Debug, Clone, Default)]
pub struct AbrFile {
    /// The `samp` tip bitmaps, in file order.
    pub samples: Vec<AbrSample>,
    /// The `desc` brushes, in file order.
    pub brushes: Vec<AbrBrush>,
    /// The `patt` patterns, in file order, decoded to RGBA8 (alpha 255).
    pub patterns: Vec<AbrPattern>,
    /// How many `Brsh` entries had a shape class other than
    /// `computedBrush` / `sampledBrush` and were skipped.
    pub skipped_shape_classes: u32,
    /// How many `patt` records were skipped (undecodable, past the
    /// per-file cap, or running past the section end). Soft: never an
    /// error.
    pub skipped_patterns: u32,
}

/// The sample a brush stamps with: the [`AbrSample`] whose id equals the
/// brush's `sampled_data_id`. `None` for a computed brush or an id no
/// sample carries.
pub fn match_sample<'a>(brush: &AbrBrush, file: &'a AbrFile) -> Option<&'a AbrSample> {
    let id = brush.sampled_data_id.as_deref()?;
    file.samples.iter().find(|sample| sample.id == id)
}

/// The pattern a brush is textured with: the [`AbrPattern`] whose id equals
/// the brush's `texture.pattern_id`. `None` for a brush without a texture
/// (or with a texture link no `patt` record carries).
pub fn match_pattern<'a>(brush: &AbrBrush, file: &'a AbrFile) -> Option<&'a AbrPattern> {
    let id = brush.texture.as_ref()?.pattern_id.as_str();
    file.patterns.iter().find(|pattern| pattern.id == id)
}

/// Whether the walk interprets the descriptor sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalkMode {
    /// `samp` only; `desc` / `phry` are skipped by length (row 268's
    /// behavior, kept for [`read_samples`]).
    SamplesOnly,
    /// `samp`, `desc` and `phry` ([`read_abr`]).
    Full,
}

/// Parses every sample (tip bitmap) in an `.abr` file.
///
/// Only `samp` sections are interpreted; every other section (`desc`,
/// `phry` and `patt` included) is skipped by its declared length — row
/// 268's exact behavior. Use [`read_abr`] for the brushes and patterns too.
/// The returned samples are in file order.
///
/// # Errors
///
/// * [`AbrError::NotAbr`] when the header or a section signature is not
///   recognizable as an `.abr` file.
/// * [`AbrError::UnsupportedVersion`] / [`AbrError::UnsupportedMinorVersion`]
///   for versions outside v6/v7 minor 1–2.
/// * [`AbrError::TooManyBrushes`] / [`AbrError::StampTooLarge`] when a
///   safety cap is exceeded (before any stamp plane is allocated).
/// * [`AbrError::Malformed`] for truncated or structurally invalid data.
/// * [`AbrError::UnsupportedBitDepth`] / [`AbrError::UnsupportedCompression`]
///   for stamp encodings this reader does not decode.
pub fn read_samples(bytes: &[u8]) -> Result<Vec<AbrSample>, AbrError> {
    walk(bytes, WalkMode::SamplesOnly).map(|file| file.samples)
}

/// Parses an `.abr` file's samples, descriptor brushes AND `patt` patterns.
///
/// `samp` sections give [`AbrFile::samples`]; each `desc` section's `Brsh`
/// list gives [`AbrFile::brushes`] (unmapped shape classes are counted in
/// [`AbrFile::skipped_shape_classes`]); `patt` sections give
/// [`AbrFile::patterns`] (undecodable records are counted in
/// [`AbrFile::skipped_patterns`], never an error); `phry` is parsed and
/// ignored; unknown sections are skipped by length.
///
/// # Errors
///
/// Every [`read_samples`] error, plus:
///
/// * [`AbrError::UnsupportedDescriptorVersion`] for a descriptor version
///   other than 16;
/// * [`AbrError::UnsupportedDescriptorType`] for an unknown OSType;
/// * [`AbrError::InvalidDescriptorUnits`] for an unknown unit code or a
///   value in the wrong units;
/// * [`AbrError::DescriptorTooLarge`] for a descriptor cap breach;
/// * [`AbrError::MalformedDescriptor`] for a truncated descriptor or a
///   descriptor section whose declared size runs past the end of the file.
pub fn read_abr(bytes: &[u8]) -> Result<AbrFile, AbrError> {
    walk(bytes, WalkMode::Full)
}

/// The shared `8BIM` section walk behind [`read_samples`] and [`read_abr`].
fn walk(bytes: &[u8], mode: WalkMode) -> Result<AbrFile, AbrError> {
    if bytes.len() < 2 {
        return Err(AbrError::NotAbr);
    }

    let mut reader = Reader::new(bytes);
    let version = reader.i16()?;
    // The versions real files use (and the ones the format authority
    // accepts): 6, 7, 9, 10. v8 is not a thing; v1-v5 predate the
    // `8BIM` section layout this reader walks.
    if !matches!(version, 6 | 7 | 9 | 10) {
        return if version >= 1 {
            Err(AbrError::UnsupportedVersion { version })
        } else {
            Err(AbrError::NotAbr)
        };
    }

    let minor = reader.i16()?;
    if minor != 1 && minor != 2 {
        return Err(AbrError::UnsupportedMinorVersion { minor });
    }
    let parameter_len = if minor == 1 { 10 } else { 264 };

    let mut file = AbrFile::default();
    // One running total of decoded plane bytes for the whole file: stamps
    // and patterns share the [`MAX_TOTAL_ALPHA_BYTES`] budget, so a file
    // cannot spend it twice over.
    let mut decoded_bytes = 0usize;
    while reader.remaining() > 0 {
        if reader.remaining() < 12 {
            return Err(malformed("truncated section header"));
        }

        let signature = reader.take(4)?;
        if signature != b"8BIM" {
            return Err(AbrError::NotAbr);
        }
        let kind = reader.take(4)?;
        let declared = reader.u32()?;
        let size = usize::try_from(declared)
            .map_err(|_| malformed("section size does not fit in memory"))?;
        let descriptor_section = mode == WalkMode::Full && (kind == b"desc" || kind == b"phry");
        if descriptor_section {
            if size > MAX_DESC_SECTION_BYTES {
                return Err(AbrError::DescriptorTooLarge {
                    what: "descriptor section bytes",
                    count: u64::from(declared),
                    max: MAX_DESC_SECTION_BYTES as u64,
                });
            }
            if size > reader.remaining() {
                return Err(AbrError::MalformedDescriptor {
                    detail: "the descriptor section runs past the end of the file".to_string(),
                });
            }
        }
        let payload = reader.take(size)?;

        if kind == b"samp" {
            parse_samp(
                payload,
                parameter_len,
                &mut file.samples,
                &mut decoded_bytes,
            )?;
        } else if descriptor_section && kind == b"desc" {
            parse_desc(payload, &mut file)?;
        } else if descriptor_section {
            // `phry`: parsed so a corrupt section is caught, then ignored
            // (the reference does the same; its meaning is undocumented).
            let _ = read_version_and_descriptor(payload)?;
        } else if mode == WalkMode::Full && kind == b"patt" {
            // Soft: a `patt` payload costs patterns, never the file. The
            // samples-only walk keeps row 268's skip-by-length.
            parse_patterns(
                payload,
                &mut file.patterns,
                &mut file.skipped_patterns,
                &mut decoded_bytes,
            );
        }

        // Sections are padded to a four-byte boundary. The trailing padding
        // may be absent at end-of-file, so clamp instead of failing.
        let pad = (4 - (size % 4)) % 4;
        let skip = pad.min(reader.remaining());
        reader.skip(skip)?;
    }

    Ok(file)
}

/// Parses one `desc` payload: a versioned descriptor whose `Brsh` list
/// holds one brush descriptor per brush.
fn parse_desc(payload: &[u8], file: &mut AbrFile) -> Result<(), AbrError> {
    let root = read_version_and_descriptor(payload)?;
    let Some(DescValue::List(entries)) = dget(&root, "Brsh") else {
        return Ok(());
    };
    for entry in entries {
        // A non-descriptor list entry carries no brush (the reference
        // ignores it too).
        let DescValue::Descriptor(brush) = entry else {
            continue;
        };
        match parse_brush(brush)? {
            Some(parsed) => file.brushes.push(parsed),
            None => file.skipped_shape_classes = file.skipped_shape_classes.saturating_add(1),
        }
    }
    Ok(())
}

/// The tool class for a brush descriptor's class id.
fn tool_class(class_id: &str) -> AbrToolClass {
    match class_id {
        "MixB" => AbrToolClass::MixerBrush,
        "SmTl" => AbrToolClass::Smudge,
        _ => AbrToolClass::Brush,
    }
}

/// A `long` as stored; other numeric types truncate (saturating), absent
/// → 0.
fn as_integer(value: Option<&DescValue>) -> i32 {
    match value {
        Some(DescValue::Integer(number)) => *number,
        other => as_number(other) as i32,
    }
}

/// Maps one brush descriptor. `Ok(None)` when its shape class is neither
/// `computedBrush` nor `sampledBrush` (the caller counts the skip).
fn parse_brush(brush: &Descriptor) -> Result<Option<AbrBrush>, AbrError> {
    let Some(shape) = as_descriptor(dget(brush, "Brsh")) else {
        return Err(AbrError::MalformedDescriptor {
            detail: "a brush has no tip-shape descriptor".to_string(),
        });
    };
    let sampled = match shape.class_id.as_str() {
        "sampledBrush" => true,
        "computedBrush" => false,
        _ => return Ok(None),
    };

    let name = as_text(dget(brush, "Nm  "))
        .or_else(|| as_text(dget(shape, "Nm  ")))
        .unwrap_or_default()
        .to_string();
    let sampled_data_id = if sampled {
        as_text(dget(shape, "sampledData")).map(str::to_string)
    } else {
        None
    };
    let dynamics = if as_bool(dget(brush, "useTipDynamics")) {
        as_descriptor(dget(brush, "szVr"))
            .map(parse_dynamics)
            .transpose()?
    } else {
        None
    };
    // The texture gate: `useTexture` alone is not enough, `Txtr` must be a
    // descriptor (the reference's rule). A `Txtr` without the gate is dead
    // data and yields `None`.
    let texture = if as_bool(dget(brush, "useTexture")) {
        as_descriptor(dget(brush, "Txtr"))
            .map(|txtr| parse_texture(brush, txtr))
            .transpose()?
    } else {
        None
    };

    Ok(Some(AbrBrush {
        tool_class: tool_class(&brush.class_id),
        name,
        diameter_px: parse_units_to_number(dget(shape, "Dmtr"), "Pixels")?,
        angle_deg: parse_angle(dget(shape, "Angl"))?,
        roundness: parse_percent(dget(shape, "Rndn"))?,
        spacing_on: as_bool(dget(shape, "Intr")),
        spacing: parse_percent(dget(shape, "Spcn"))?,
        flip_x: as_bool(dget(shape, "flipX")),
        flip_y: as_bool(dget(shape, "flipY")),
        hardness: parse_percent(dget(shape, "Hrdn"))?,
        sampled_data_id,
        dynamics,
        texture,
    }))
}

/// Maps one texture: the link descriptor `Txtr` (`Idnt`, `Nm  `) plus the
/// brush-level parameters that shape it.
fn parse_texture(brush: &Descriptor, txtr: &Descriptor) -> Result<AbrTexture, AbrError> {
    Ok(AbrTexture {
        pattern_id: as_text(dget(txtr, "Idnt")).unwrap_or_default().to_string(),
        pattern_name: as_text(dget(txtr, "Nm  ")).unwrap_or_default().to_string(),
        scale: parse_percent(dget(brush, "textureScale"))?,
        depth: parse_percent(dget(brush, "textureDepth"))?,
        invert: as_bool(dget(brush, "InvT")),
        blend_mode: as_text(dget(brush, "textureBlendMode"))
            .unwrap_or_default()
            .to_string(),
        each_tip: as_bool(dget(brush, "TxtC")),
    })
}

/// Maps one dynamics descriptor (`bVTy` / `fStp` / `jitter` / `Mnm `).
fn parse_dynamics(desc: &Descriptor) -> Result<AbrDynamics, AbrError> {
    Ok(AbrDynamics {
        control_index: as_integer(dget(desc, "bVTy")),
        steps: as_integer(dget(desc, "fStp")),
        jitter: parse_percent(dget(desc, "jitter"))?,
        minimum: parse_percent(dget(desc, "Mnm "))?,
    })
}

/// Parses the concatenated brush records of one `samp` payload, charging
/// decoded planes against the file's running total (shared with `patt`).
fn parse_samp(
    payload: &[u8],
    parameter_len: usize,
    samples: &mut Vec<AbrSample>,
    total_bytes: &mut usize,
) -> Result<(), AbrError> {
    parse_samp_into(
        payload,
        parameter_len,
        samples,
        total_bytes,
        MAX_TOTAL_ALPHA_BYTES,
    )
}

/// [`parse_samp`] with an explicit total-plane budget (the test seam for
/// the cap; production always passes [`MAX_TOTAL_ALPHA_BYTES`]).
#[cfg(test)]
fn parse_samp_with_budget(
    payload: &[u8],
    parameter_len: usize,
    samples: &mut Vec<AbrSample>,
    max_total_bytes: usize,
) -> Result<(), AbrError> {
    let mut total_bytes = 0usize;
    parse_samp_into(
        payload,
        parameter_len,
        samples,
        &mut total_bytes,
        max_total_bytes,
    )
}

/// The shared body of [`parse_samp`] and [`parse_samp_with_budget`].
fn parse_samp_into(
    payload: &[u8],
    parameter_len: usize,
    samples: &mut Vec<AbrSample>,
    total_bytes: &mut usize,
    max_total_bytes: usize,
) -> Result<(), AbrError> {
    let mut reader = Reader::new(payload);
    while reader.remaining() > 0 {
        if samples.len() >= MAX_SAMPLES {
            return Err(AbrError::TooManyBrushes {
                count: samples.len() + 1,
                max: MAX_SAMPLES,
            });
        }
        if reader.remaining() < 4 {
            return Err(malformed("truncated brush record length"));
        }

        let declared = usize::try_from(reader.u32()?)
            .map_err(|_| malformed("brush record length does not fit in memory"))?;
        if declared == 0 {
            return Err(malformed("zero-length brush record"));
        }
        let pad = (4 - (declared % 4)) % 4;
        let padded = declared
            .checked_add(pad)
            .ok_or_else(|| malformed("brush record length overflow"))?;
        let body = reader.take(padded)?;
        let sample = parse_sample(body, parameter_len)?;
        // The total-plane budget: the per-stamp cap alone still allows
        // 4096 stamps of 16 MiB each (64 GiB of decoded planes from a
        // small hostile file), so the SUM is capped too. The same total
        // carries the pattern planes.
        *total_bytes = total_bytes
            .checked_add(sample.alpha.len())
            .ok_or_else(|| malformed("total stamp data size overflow"))?;
        if *total_bytes > max_total_bytes {
            return Err(AbrError::TooMuchData {
                bytes: *total_bytes,
                max: max_total_bytes,
            });
        }
        samples.push(sample);
    }
    Ok(())
}

/// Parses one brush record body (everything after its length prefix).
fn parse_sample(body: &[u8], parameter_len: usize) -> Result<AbrSample, AbrError> {
    let mut reader = Reader::new(body);

    let id_len = usize::from(reader.u8()?);
    let id_bytes = reader.take(id_len)?;
    let id: String = id_bytes.iter().map(|&byte| byte as char).collect();

    // The v6.1 tip-shape block is five big-endian u16s; v6.2 widens the
    // block to 264 bytes but (per the module docs) keeps the same prefix.
    // The block is SKIPPED, not interpreted: the reference implementation
    // skips it without naming its fields, and the brush's real settings
    // (diameter, hardness, spacing, angle) live in the descriptor
    // (`desc`) sections, which this parser does not read yet (the named
    // follow-up wave). Reading the padding as parameters would publish
    // numbers that are not the brush's, so this parser refuses to guess.
    let _parameters = reader.take(parameter_len)?;

    // Bounds: y, x, bottom, right. Height and width are the spans; x
    // and y are the sample's absolute position in the file's space.
    let y = i64::from(reader.i32()?);
    let x = i64::from(reader.i32()?);
    let bottom = i64::from(reader.i32()?);
    let right = i64::from(reader.i32()?);
    let height_span = bottom - y;
    let width_span = right - x;
    if width_span <= 0 || height_span <= 0 {
        return Err(malformed("brush stamp has non-positive bounds"));
    }

    let width =
        u32::try_from(width_span).map_err(|_| malformed("brush stamp width is out of range"))?;
    let height =
        u32::try_from(height_span).map_err(|_| malformed("brush stamp height is out of range"))?;
    if width > MAX_STAMP_DIMENSION || height > MAX_STAMP_DIMENSION {
        return Err(AbrError::StampTooLarge {
            width,
            height,
            max: MAX_STAMP_DIMENSION,
        });
    }

    // The cap is enforced above, so this allocation is bounded.
    let width_px = usize::try_from(width)
        .map_err(|_| malformed("brush stamp width does not fit in memory"))?;
    let height_px = usize::try_from(height)
        .map_err(|_| malformed("brush stamp height does not fit in memory"))?;
    let pixel_count = width_px
        .checked_mul(height_px)
        .ok_or_else(|| malformed("brush stamp size overflow"))?;
    let mut alpha = vec![0u8; pixel_count];

    let bit_depth = reader.i16()?;
    let compression = reader.u8()?;
    match (bit_depth, compression) {
        (8, 0) => {
            let raw = reader.take(pixel_count)?;
            alpha.copy_from_slice(raw);
        }
        (8, 1) => decode_rle(&mut reader, width_px, height_px, &mut alpha)?,
        (16, 0) => {
            let byte_count = pixel_count
                .checked_mul(2)
                .ok_or_else(|| malformed("16-bit brush stamp size overflow"))?;
            let raw = reader.take(byte_count)?;
            for (pixel, sample) in alpha.iter_mut().zip(raw.chunks_exact(2)) {
                if let [high, _low] = sample {
                    *pixel = *high;
                }
            }
        }
        (8 | 16, other) => {
            return Err(AbrError::UnsupportedCompression { compression: other });
        }
        (depth, _) => return Err(AbrError::UnsupportedBitDepth { depth }),
    }

    Ok(AbrSample {
        id,
        x: i32::try_from(x).unwrap_or(i32::MIN),
        y: i32::try_from(y).unwrap_or(i32::MIN),
        width,
        height,
        alpha,
    })
}

/// Decodes a one-plane, 8-bit RLE stamp.
///
/// The row length table (one big-endian `u16` per row) precedes the packed
/// rows. Each row decodes into `width` bytes; short rows leave zeros.
fn decode_rle(
    reader: &mut Reader<'_>,
    width: usize,
    height: usize,
    alpha: &mut [u8],
) -> Result<(), AbrError> {
    let mut lengths = Vec::with_capacity(height);
    for _ in 0..height {
        lengths.push(usize::from(reader.u16()?));
    }

    for (row, &encoded_len) in alpha.chunks_exact_mut(width).zip(lengths.iter()) {
        let encoded = reader.take(encoded_len)?;
        decode_packbits_row(encoded, row)?;
    }
    Ok(())
}

/// Decodes one PackBits row into `out` (which is the row's `width` slice).
///
/// Header `0..=127` is a literal run of `header + 1` bytes; `129..=255`
/// repeats the next byte `257 - header` times; `128` is a no-op. This is
/// exactly the reference's `read_data_rle` behavior. `pub(crate)` so the
/// `patt` decoder (`abr_patt.rs`) reuses this one PackBits implementation.
pub(crate) fn decode_packbits_row(encoded: &[u8], out: &mut [u8]) -> Result<(), AbrError> {
    let mut index = 0usize;
    let mut written = 0usize;

    while let Some(&header) = encoded.get(index) {
        index += 1;
        match header {
            0..=127 => {
                let run = usize::from(header) + 1;
                let end = index
                    .checked_add(run)
                    .ok_or_else(|| malformed("RLE literal run overflow"))?;
                let literal = encoded
                    .get(index..end)
                    .ok_or_else(|| malformed("truncated RLE literal run"))?;
                let take = run.min(out.len().saturating_sub(written));
                if let Some(destination) = out.get_mut(written..written + take) {
                    if let Some(source) = literal.get(..take) {
                        destination.copy_from_slice(source);
                    }
                }
                written += take;
                index = end;
            }
            129..=255 => {
                let Some(&value) = encoded.get(index) else {
                    return Err(malformed("truncated RLE repeat run"));
                };
                index += 1;
                let run = 257 - usize::from(header);
                let take = run.min(out.len().saturating_sub(written));
                if let Some(destination) = out.get_mut(written..written + take) {
                    destination.fill(value);
                }
                written += take;
            }
            _ => {
                // 128: PackBits' no-op; consume only the header byte.
            }
        }
    }

    Ok(())
}

/// Builds a [`AbrError::Malformed`] with a static, user-facing detail.
pub(crate) fn malformed(detail: impl Into<String>) -> AbrError {
    AbrError::Malformed {
        detail: detail.into(),
    }
}

/// A bounds-checked, big-endian cursor over the input bytes.
///
/// `pub(crate)` so the `patt` decoder (`abr_patt.rs`) reads its records with
/// the same bounds-checked primitives as the `samp` reader.
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8], AbrError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| malformed("record length overflow"))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| malformed("unexpected end of file"))?;
        self.offset = end;
        Ok(slice)
    }

    pub(crate) fn skip(&mut self, len: usize) -> Result<(), AbrError> {
        self.take(len).map(|_| ())
    }

    pub(crate) fn u8(&mut self) -> Result<u8, AbrError> {
        match self.take(1)? {
            [byte] => Ok(*byte),
            _ => Err(malformed("unexpected end of file")),
        }
    }

    pub(crate) fn u16(&mut self) -> Result<u16, AbrError> {
        match self.take(2)? {
            [high, low] => Ok(u16::from_be_bytes([*high, *low])),
            _ => Err(malformed("unexpected end of file")),
        }
    }

    pub(crate) fn i16(&mut self) -> Result<i16, AbrError> {
        match self.take(2)? {
            [high, low] => Ok(i16::from_be_bytes([*high, *low])),
            _ => Err(malformed("unexpected end of file")),
        }
    }

    pub(crate) fn u32(&mut self) -> Result<u32, AbrError> {
        match self.take(4)? {
            [a, b, c, d] => Ok(u32::from_be_bytes([*a, *b, *c, *d])),
            _ => Err(malformed("unexpected end of file")),
        }
    }

    pub(crate) fn i32(&mut self) -> Result<i32, AbrError> {
        match self.take(4)? {
            [a, b, c, d] => Ok(i32::from_be_bytes([*a, *b, *c, *d])),
            _ => Err(malformed("unexpected end of file")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abr_desc::{MAX_DESC_DEPTH, MAX_TEXT_CHARS};

    /// Minimal big-endian writer for the synthetic fixtures.
    struct Builder {
        bytes: Vec<u8>,
    }

    impl Builder {
        fn new() -> Self {
            Self { bytes: Vec::new() }
        }

        fn u8(&mut self, value: u8) {
            self.bytes.push(value);
        }

        fn u16(&mut self, value: u16) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn i16(&mut self, value: i16) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn u32(&mut self, value: u32) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn i32(&mut self, value: i32) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn raw(&mut self, bytes: &[u8]) {
            self.bytes.extend_from_slice(bytes);
        }

        fn pascal(&mut self, text: &str) {
            let bytes = text.as_bytes();
            let len = u8::try_from(bytes.len()).expect("test brush name fits a u8");
            self.u8(len);
            self.raw(bytes);
        }

        fn into_vec(self) -> Vec<u8> {
            self.bytes
        }
    }

    /// One descriptor value for the synthetic `desc` fixtures.
    enum Val {
        Text(&'static str),
        Units(&'static [u8; 4], f64),
        Bool(bool),
        Long(i32),
        Enum(&'static str, &'static str),
        Obj(&'static str, Vec<(&'static str, Val)>),
        List(Vec<Val>),
    }

    /// The descriptor writers (big-endian, the layout `abr_desc` reads).
    impl Builder {
        fn f64(&mut self, value: f64) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        /// `u32` code-unit count + big-endian UTF-16 units.
        fn unicode(&mut self, text: &str) {
            let units: Vec<u16> = text.encode_utf16().collect();
            self.u32(u32::try_from(units.len()).expect("test text fits a u32"));
            for unit in units {
                self.u16(unit);
            }
        }

        /// ascii-or-class-id: a four-byte id as `0` + the id, otherwise
        /// `i32` length + bytes.
        fn id(&mut self, id: &str) {
            if id.len() == 4 {
                self.i32(0);
            } else {
                self.i32(i32::try_from(id.len()).expect("test id fits an i32"));
            }
            self.raw(id.as_bytes());
        }

        /// A type signature + its value.
        fn value(&mut self, value: &Val) {
            match value {
                Val::Text(text) => {
                    self.raw(b"TEXT");
                    self.unicode(text);
                }
                Val::Units(code, number) => {
                    self.raw(b"UntF");
                    self.raw(*code);
                    self.f64(*number);
                }
                Val::Bool(flag) => {
                    self.raw(b"bool");
                    self.u8(u8::from(*flag));
                }
                Val::Long(number) => {
                    self.raw(b"long");
                    self.i32(*number);
                }
                Val::Enum(kind, value) => {
                    self.raw(b"enum");
                    self.id(kind);
                    self.id(value);
                }
                Val::Obj(class_id, items) => {
                    self.raw(b"Objc");
                    self.descriptor_body(class_id, items);
                }
                Val::List(items) => {
                    self.raw(b"VlLs");
                    self.i32(i32::try_from(items.len()).expect("test list fits an i32"));
                    for item in items {
                        self.value(item);
                    }
                }
            }
        }

        /// Class structure (empty name + class id) + item count + items.
        fn descriptor_body(&mut self, class_id: &str, items: &[(&'static str, Val)]) {
            self.unicode("");
            self.id(class_id);
            self.u32(u32::try_from(items.len()).expect("test items fit a u32"));
            for (key, value) in items {
                self.id(key);
                self.value(value);
            }
        }
    }

    /// A versioned (16) root descriptor of class `null` with `items`.
    fn root_payload(items: &[(&'static str, Val)]) -> Vec<u8> {
        let mut payload = Builder::new();
        payload.u32(16);
        payload.descriptor_body("null", items);
        payload.into_vec()
    }

    /// A `desc` payload whose `Brsh` list holds `entries`.
    fn desc_payload(entries: Vec<Val>) -> Vec<u8> {
        root_payload(&[("Brsh", Val::List(entries))])
    }

    /// The 4x4 raw tip the descriptor fixtures stamp with.
    const TIP_ALPHA: [u8; 16] = [
        0, 17, 34, 51, 68, 85, 102, 119, 136, 153, 170, 187, 204, 221, 238, 255,
    ];

    /// One raw 8-bit 4x4 `samp` record with id `tip1`.
    fn tip_record() -> Vec<u8> {
        let mut stamp = Stamp::new("tip1", 4, 4, &TIP_ALPHA);
        stamp.compression = 0;
        brush_record(&stamp)
    }

    /// A brush descriptor (outer class `outer`) with a `sampledBrush`
    /// shape linked to `tip_id`, and size dynamics on.
    fn sampled_brush(outer: &'static str, tip_id: &'static str) -> Val {
        Val::Obj(
            outer,
            vec![
                (
                    "Brsh",
                    Val::Obj(
                        "sampledBrush",
                        vec![
                            ("Nm  ", Val::Text("Test Tip")),
                            ("Dmtr", Val::Units(b"#Pxl", 32.0)),
                            ("Angl", Val::Units(b"#Ang", 15.0)),
                            ("Rndn", Val::Units(b"#Prc", 100.0)),
                            ("Intr", Val::Bool(true)),
                            ("Spcn", Val::Units(b"#Prc", 25.0)),
                            ("Hrdn", Val::Units(b"#Prc", 80.0)),
                            ("flipX", Val::Bool(false)),
                            ("flipY", Val::Bool(false)),
                            ("sampledData", Val::Text(tip_id)),
                        ],
                    ),
                ),
                ("useTipDynamics", Val::Bool(true)),
                (
                    "szVr",
                    Val::Obj(
                        "brVr",
                        vec![
                            ("bVTy", Val::Long(1)),
                            ("fStp", Val::Long(4)),
                            ("jitter", Val::Units(b"#Prc", 10.0)),
                            ("Mnm ", Val::Units(b"#Prc", 0.0)),
                        ],
                    ),
                ),
            ],
        )
    }

    /// The exact brush [`sampled_brush`] must parse to.
    fn expected_sampled(tool_class: AbrToolClass, tip_id: &str) -> AbrBrush {
        AbrBrush {
            tool_class,
            name: "Test Tip".to_string(),
            diameter_px: 32.0,
            angle_deg: 15.0,
            roundness: 1.0,
            spacing_on: true,
            spacing: 0.25,
            flip_x: false,
            flip_y: false,
            hardness: 0.8,
            sampled_data_id: Some(tip_id.to_string()),
            dynamics: Some(AbrDynamics {
                control_index: 1,
                steps: 4,
                jitter: 0.1,
                minimum: 0.0,
            }),
            texture: None,
        }
    }

    /// The variable parts of one synthetic sampled brush.
    struct Stamp<'a> {
        name: &'a str,
        width: i32,
        height: i32,
        depth: i16,
        compression: u8,
        payload: &'a [u8],
        hardness_percent: u16,
        spacing_percent: u16,
        diameter: u16,
    }

    impl<'a> Stamp<'a> {
        fn new(name: &'a str, width: i32, height: i32, payload: &'a [u8]) -> Self {
            Self {
                name,
                width,
                height,
                depth: 8,
                compression: 1,
                payload,
                hardness_percent: 100,
                spacing_percent: 25,
                diameter: 0,
            }
        }
    }

    /// Encodes a minor-1 brush record: length prefix, name, the 10-byte
    /// parameter block, bounds, depth, compression, alpha payload.
    fn brush_record(stamp: &Stamp<'_>) -> Vec<u8> {
        let mut body = Builder::new();
        body.pascal(stamp.name);
        body.u16(0); // roundness (ignored by the parser)
        body.u16(0); // angle (ignored by the parser)
        body.u16(stamp.hardness_percent);
        body.u16(stamp.spacing_percent);
        body.u16(stamp.diameter);
        body.i32(0); // y
        body.i32(0); // x
        body.i32(stamp.height); // bottom
        body.i32(stamp.width); // right
        body.i16(stamp.depth);
        body.u8(stamp.compression);
        body.raw(stamp.payload);

        let mut body = body.into_vec();
        let declared = u32::try_from(body.len()).expect("test brush body fits a u32");
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }

        let mut record = Builder::new();
        record.u32(declared);
        record.raw(&body);
        record.into_vec()
    }

    /// Wraps section payloads in a v6 file with the given minor version.
    fn abr_file(minor: i16, sections: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut file = Builder::new();
        file.i16(6);
        file.i16(minor);
        for (kind, payload) in sections {
            file.raw(b"8BIM");
            file.raw(*kind);
            file.u32(u32::try_from(payload.len()).expect("test payload fits a u32"));
            file.raw(payload);
            let pad = (4 - payload.len() % 4) % 4;
            for _ in 0..pad {
                file.u8(0);
            }
        }
        file.into_vec()
    }

    /// Builds the RLE payload: one big-endian u16 length per row, then the
    /// packed rows.
    fn rle_payload(rows: &[Vec<u8>]) -> Vec<u8> {
        let mut payload = Builder::new();
        for row in rows {
            payload.u16(u16::try_from(row.len()).expect("test row fits a u16"));
        }
        for row in rows {
            payload.raw(row);
        }
        payload.into_vec()
    }

    /// Pin 1: a synthetic v6 file yields exactly the written brush, with
    /// the alpha plane equal byte-for-byte.
    #[test]
    fn reads_a_synthetic_v6_brush_byte_for_byte() {
        let alpha = [10u8, 20, 30, 40, 50, 60, 70, 80];
        let rows = vec![
            vec![3u8, 10, 20, 30, 40], // literal run of four
            vec![3u8, 50, 60, 70, 80],
        ];
        let payload = rle_payload(&rows);

        let mut stamp = Stamp::new("Test Tip", 4, 2, &payload);
        stamp.hardness_percent = 50;
        stamp.spacing_percent = 25;
        stamp.diameter = 12;
        let record = brush_record(&stamp);
        let file = abr_file(1, &[(b"samp", record)]);

        let samples = read_samples(&file).expect("synthetic v6 file parses");
        assert_eq!(samples.len(), 1);
        let sample = &samples[0];
        assert_eq!(sample.id, "Test Tip");
        assert_eq!(sample.width, 4);
        assert_eq!(sample.height, 2);
        assert_eq!(sample.alpha, alpha);
    }

    /// Pin 2: one row mixing a literal run and a repeat run decodes to the
    /// hand-computed bytes.
    #[test]
    fn packbits_literal_plus_repeat_matches_hand_computed_bytes() {
        // header 2 = three literal bytes; header 0xFE = repeat three times.
        let rows = vec![vec![2u8, 1, 2, 3, 0xFE, 9]];
        let payload = rle_payload(&rows);
        let stamp = Stamp::new("Rle", 6, 1, &payload);
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        let samples = read_samples(&file).expect("RLE fixture parses");
        assert_eq!(samples[0].alpha, vec![1, 2, 3, 9, 9, 9]);
    }

    /// Pin 3: the per-brush parameter block is SKIPPED, never published —
    /// the brush's real settings live in the descriptor sections, which
    /// this parser does not read, so whatever bytes sit in the block
    /// cannot change the parsed stamp.
    #[test]
    fn parameter_block_is_skipped_not_interpreted() {
        let alpha = [128u8];
        let mut stamp = Stamp::new("Half", 1, 1, &alpha);
        stamp.compression = 0;
        stamp.hardness_percent = 50;
        stamp.spacing_percent = 50;
        stamp.diameter = 9;
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        let samples = read_samples(&file).expect("percent fixture parses");
        // The stamp is exact; the block's numbers are NOT exposed.
        assert_eq!(samples[0].id, "Half");
        assert_eq!(samples[0].width, 1);
        assert_eq!(samples[0].height, 1);
        assert_eq!(samples[0].alpha, alpha);
        // A file whose block holds different bytes parses to the SAME
        // stamp: the block carries no published data.
        let mut other = Stamp::new("Half", 1, 1, &alpha);
        other.compression = 0;
        other.hardness_percent = 250;
        other.spacing_percent = 250;
        other.diameter = 4096;
        let other_file = abr_file(1, &[(b"samp", brush_record(&other))]);
        let other_samples = read_samples(&other_file).expect("variant fixture parses");
        assert_eq!(other_samples[0].alpha, samples[0].alpha);
        assert_eq!(other_samples[0].width, samples[0].width);
    }

    /// Pin 4: wrong magic bytes give the named "not an .abr" error.
    #[test]
    fn wrong_magic_is_not_an_abr() {
        assert_eq!(read_samples(&[]).unwrap_err(), AbrError::NotAbr);
        assert_eq!(
            read_samples(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap_err(),
            AbrError::NotAbr
        );

        // A plausible v6 header followed by a corrupt section signature.
        let mut file = Builder::new();
        file.i16(6);
        file.i16(1);
        file.raw(b"8BAD");
        file.raw(b"samp");
        file.u32(0);
        assert_eq!(
            read_samples(&file.into_vec()).unwrap_err(),
            AbrError::NotAbr
        );
    }

    /// Pin 5: a file cut inside the alpha payload is malformed, never a
    /// panic and never a partial success.
    #[test]
    fn truncated_alpha_data_is_malformed() {
        let rows = vec![vec![3u8, 1, 2, 3, 4]];
        let payload = rle_payload(&rows);
        let stamp = Stamp::new("Cut", 4, 1, &payload);
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        // Drop the last four bytes (the tail of the alpha payload plus the
        // record padding). Length-delimited parsing reports the truncation
        // before it can produce a brush.
        let cut = file.len() - 4;
        let error = read_samples(&file[..cut]).unwrap_err();
        assert!(
            matches!(error, AbrError::Malformed { .. }),
            "expected Malformed, got {error:?}"
        );
    }

    /// Pin 6: an oversized declared stamp trips the cap before the plane is
    /// sized. The declared size here is 100_000 x 100_000 = 10 GB, so a
    /// pre-allocation check is the only way this test can finish.
    #[test]
    fn oversized_stamp_is_rejected_before_allocating() {
        let mut body = Builder::new();
        body.pascal("Huge");
        body.u16(0);
        body.u16(0);
        body.u16(100);
        body.u16(25);
        body.u16(9);
        body.i32(0); // y
        body.i32(0); // x
        body.i32(100_000); // bottom -> height
        body.i32(100_000); // right  -> width
        body.i16(8);
        body.u8(1); // RLE, but the cap must fire before any alpha read
        let mut body = body.into_vec();
        let declared = u32::try_from(body.len()).expect("test body fits a u32");
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }
        let mut record = Builder::new();
        record.u32(declared);
        record.raw(&body);
        let file = abr_file(1, &[(b"samp", record.into_vec())]);

        let error = read_samples(&file).unwrap_err();
        assert_eq!(
            error,
            AbrError::StampTooLarge {
                width: 100_000,
                height: 100_000,
                max: MAX_STAMP_DIMENSION,
            }
        );
    }

    /// Pin 7: a v8+ descriptor-only file names its version.
    #[test]
    fn descriptor_only_v8_is_unsupported() {
        let mut file = Builder::new();
        file.i16(8);
        file.i16(2);
        file.raw(b"8BIM");
        file.raw(b"desc");
        file.u32(0);

        assert_eq!(
            read_samples(&file.into_vec()).unwrap_err(),
            AbrError::UnsupportedVersion { version: 8 }
        );
    }

    /// v1/v2/v5 (and every other pre-v6 version) are named unsupported.
    #[test]
    fn older_versions_are_unsupported() {
        for version in [1i16, 2, 5] {
            let mut file = Builder::new();
            file.i16(version);
            file.i16(1);
            assert_eq!(
                read_samples(&file.into_vec()).unwrap_err(),
                AbrError::UnsupportedVersion { version }
            );
        }
    }

    /// A minor version outside 1..=2 is named unsupported.
    #[test]
    fn unknown_minor_version_is_unsupported() {
        let mut file = Builder::new();
        file.i16(6);
        file.i16(3);
        assert_eq!(
            read_samples(&file.into_vec()).unwrap_err(),
            AbrError::UnsupportedMinorVersion { minor: 3 }
        );
    }

    /// Raw 8-bit alpha is copied verbatim.
    #[test]
    fn raw_alpha_is_copied_verbatim() {
        let alpha = [7u8, 8, 9, 10, 11, 12];
        let mut stamp = Stamp::new("Raw", 3, 2, &alpha);
        stamp.compression = 0;
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        let samples = read_samples(&file).expect("raw fixture parses");
        assert_eq!(samples[0].alpha, alpha);
        assert_eq!(samples[0].width, 3);
        assert_eq!(samples[0].height, 2);
    }

    /// 16-bit raw samples keep their high byte (big-endian).
    #[test]
    fn sixteen_bit_raw_keeps_the_high_byte() {
        let raw = [0x12u8, 0x34, 0xAB, 0xCD];
        let mut stamp = Stamp::new("Deep", 2, 1, &raw);
        stamp.compression = 0;
        stamp.depth = 16;
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        let samples = read_samples(&file).expect("16-bit fixture parses");
        assert_eq!(samples[0].alpha, vec![0x12, 0xAB]);
    }

    /// PackBits header 128 is a no-op, matching the reference.
    #[test]
    fn packbits_128_is_a_noop() {
        let rows = vec![vec![2u8, 1, 2, 3, 0x80, 0xFE, 9]];
        let payload = rle_payload(&rows);
        let stamp = Stamp::new("Noop", 6, 1, &payload);
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        let samples = read_samples(&file).expect("no-op fixture parses");
        assert_eq!(samples[0].alpha, vec![1, 2, 3, 9, 9, 9]);
    }

    /// Non-`samp` sections are skipped by their declared length, so a
    /// descriptor section before the stamps does not break the read.
    #[test]
    fn descriptor_section_is_skipped() {
        let descriptor = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02];
        let alpha = [200u8];
        let mut stamp = Stamp::new("After Desc", 1, 1, &alpha);
        stamp.compression = 0;
        let record = brush_record(&stamp);
        let file = abr_file(1, &[(b"desc", descriptor), (b"samp", record)]);

        let samples = read_samples(&file).expect("desc-prefixed file parses");
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].id, "After Desc");
        assert_eq!(samples[0].alpha, vec![200]);
    }

    /// The brush-count cap fires at 4097 records.
    #[test]
    fn more_than_max_brushes_is_rejected() {
        let mut payload = Vec::new();
        for index in 0..=MAX_SAMPLES {
            let name = format!("b{index}");
            let alpha = [index as u8];
            let mut stamp = Stamp::new(&name, 1, 1, &alpha);
            stamp.compression = 0;
            payload.extend_from_slice(&brush_record(&stamp));
        }
        let file = abr_file(1, &[(b"samp", payload)]);

        assert_eq!(
            read_samples(&file).unwrap_err(),
            AbrError::TooManyBrushes {
                count: MAX_SAMPLES + 1,
                max: MAX_SAMPLES,
            }
        );
    }

    /// A minor-2 record is structurally identical, with a 264-byte
    /// parameter block.
    #[test]
    fn minor_two_reads_the_parameter_prefix() {
        let mut body = Builder::new();
        body.pascal("V62");
        body.u16(0); // roundness
        body.u16(0); // angle
        body.u16(40); // hardness %
        body.u16(30); // spacing %
        body.u16(18); // diameter
        for _ in 0..254 {
            body.u8(0); // the rest of the v6.2 block
        }
        body.i32(0);
        body.i32(0);
        body.i32(1);
        body.i32(1);
        body.i16(8);
        body.u8(0);
        body.u8(77);
        let mut body = body.into_vec();
        let declared = u32::try_from(body.len()).expect("test body fits a u32");
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }
        let mut record = Builder::new();
        record.u32(declared);
        record.raw(&body);
        let file = abr_file(2, &[(b"samp", record.into_vec())]);

        let samples = read_samples(&file).expect("minor-2 fixture parses");
        assert_eq!(samples[0].id, "V62");
        assert_eq!(samples[0].alpha, vec![77]);
        assert_eq!(samples[0].width, 1);
        assert_eq!(samples[0].height, 1);
    }

    /// The total-plane budget refuses a file whose decoded stamps exceed
    /// the cap (the per-stamp cap alone allows 64 GiB across 4096
    /// stamps).
    #[test]
    fn total_data_budget_refuses_the_sum() {
        let alpha = [0u8, 1, 2, 3];
        let mut stamp = Stamp::new("Hot", 4, 1, &alpha);
        stamp.compression = 0;
        let payload = brush_record(&stamp);
        let mut brushes = Vec::new();
        let err = parse_samp_with_budget(&payload, 10, &mut brushes, 3)
            .expect_err("the sum over the budget is refused");
        assert_eq!(err, AbrError::TooMuchData { bytes: 4, max: 3 });
        assert!(brushes.is_empty(), "the refused brush is not published");
    }

    /// Version 7 shares the v6 `samp` layout.
    #[test]
    fn version_seven_is_accepted() {
        let alpha = [5u8];
        let mut stamp = Stamp::new("V7", 1, 1, &alpha);
        stamp.compression = 0;
        let mut file = abr_file(1, &[(b"samp", brush_record(&stamp))]);
        file[0..2].copy_from_slice(&7i16.to_be_bytes());

        let samples = read_samples(&file).expect("v7 fixture parses");
        assert_eq!(samples[0].id, "V7");
        assert_eq!(samples[0].alpha, vec![5]);
    }

    /// A zero-length brush record is rejected instead of looping forever.
    #[test]
    fn zero_length_brush_record_is_malformed() {
        let mut payload = Builder::new();
        payload.u32(0);
        let file = abr_file(1, &[(b"samp", payload.into_vec())]);

        let error = read_samples(&file).unwrap_err();
        assert!(
            matches!(error, AbrError::Malformed { .. }),
            "expected Malformed, got {error:?}"
        );
    }

    /// Row 282 pin: one samp + one desc holding one sampledBrush parse to
    /// the exact written fields, and the brush matches its tip by id.
    #[test]
    fn reads_a_sampled_brush_and_matches_its_tip() {
        let desc = desc_payload(vec![sampled_brush("_", "tip1")]);
        let file = abr_file(1, &[(b"samp", tip_record()), (b"desc", desc)]);

        let abr = read_abr(&file).expect("the synthetic desc file parses");
        assert_eq!(abr.samples.len(), 1);
        assert_eq!(abr.skipped_shape_classes, 0);
        assert_eq!(
            abr.brushes,
            vec![expected_sampled(AbrToolClass::Brush, "tip1")]
        );

        let tip = match_sample(&abr.brushes[0], &abr).expect("the brush finds its tip");
        assert_eq!(tip.id, "tip1");
        assert_eq!(tip.width, 4);
        assert_eq!(tip.height, 4);
        assert_eq!(tip.alpha, TIP_ALPHA);
    }

    /// A computedBrush has no sampled id (and takes the angle default);
    /// a dBrush entry in the same list is skipped and counted.
    #[test]
    fn computed_brush_has_no_sample_and_dbrush_is_skipped() {
        let computed = Val::Obj(
            "SmTl",
            vec![
                ("Nm  ", Val::Text("Hard Round")),
                (
                    "Brsh",
                    Val::Obj(
                        "computedBrush",
                        vec![
                            ("Dmtr", Val::Units(b"#Pxl", 12.0)),
                            ("Rndn", Val::Units(b"#Prc", 50.0)),
                            ("Intr", Val::Bool(true)),
                            ("Spcn", Val::Units(b"#Prc", 10.0)),
                            ("Hrdn", Val::Units(b"#Prc", 100.0)),
                            ("flipX", Val::Bool(true)),
                            ("flipY", Val::Bool(false)),
                        ],
                    ),
                ),
            ],
        );
        let dbrush_shape = vec![("Dmtr", Val::Units(b"#Pxl", 5.0))];
        let dynamic = Val::Obj("_", vec![("Brsh", Val::Obj("dBrush", dbrush_shape))]);
        let file = abr_file(1, &[(b"desc", desc_payload(vec![computed, dynamic]))]);

        let abr = read_abr(&file).expect("the computed fixture parses");
        assert_eq!(abr.skipped_shape_classes, 1);
        assert_eq!(
            abr.brushes,
            vec![AbrBrush {
                tool_class: AbrToolClass::Smudge,
                name: "Hard Round".to_string(),
                diameter_px: 12.0,
                angle_deg: 0.0,
                roundness: 0.5,
                spacing_on: true,
                spacing: 0.1,
                flip_x: true,
                flip_y: false,
                hardness: 1.0,
                sampled_data_id: None,
                dynamics: None,
                texture: None,
            }]
        );
        assert!(match_sample(&abr.brushes[0], &abr).is_none());
    }

    /// Two sampledBrush entries in one desc give two brushes, in order,
    /// with their own tool classes.
    #[test]
    fn two_sampled_brushes_give_two_entries() {
        let entries = vec![sampled_brush("_", "tip1"), sampled_brush("MixB", "tip2")];
        let file = abr_file(1, &[(b"desc", desc_payload(entries))]);

        let abr = read_abr(&file).expect("the two-brush fixture parses");
        assert_eq!(
            abr.brushes,
            vec![
                expected_sampled(AbrToolClass::Brush, "tip1"),
                expected_sampled(AbrToolClass::MixerBrush, "tip2"),
            ]
        );
    }

    /// A sampledData id no sample carries matches nothing.
    #[test]
    fn unmatched_sampled_data_id_is_none() {
        let desc = desc_payload(vec![sampled_brush("_", "missing")]);
        let file = abr_file(1, &[(b"samp", tip_record()), (b"desc", desc)]);

        let abr = read_abr(&file).expect("the unmatched fixture parses");
        assert_eq!(abr.brushes.len(), 1);
        assert!(match_sample(&abr.brushes[0], &abr).is_none());
    }

    /// A descriptor version other than 16 is refused by name.
    #[test]
    fn descriptor_version_fifteen_is_unsupported() {
        let mut payload = Builder::new();
        payload.u32(15);
        payload.descriptor_body("null", &[]);
        let file = abr_file(1, &[(b"desc", payload.into_vec())]);

        assert_eq!(
            read_abr(&file).unwrap_err(),
            AbrError::UnsupportedDescriptorVersion(15)
        );
    }

    /// An OSType outside the table is a named strict error.
    #[test]
    fn unknown_ostype_is_unsupported() {
        let mut payload = Builder::new();
        payload.u32(16);
        payload.unicode("");
        payload.id("null");
        payload.u32(1);
        payload.id("abcd");
        payload.raw(b"zzzz");
        let file = abr_file(1, &[(b"desc", payload.into_vec())]);

        assert_eq!(
            read_abr(&file).unwrap_err(),
            AbrError::UnsupportedDescriptorType("zzzz".to_string())
        );
    }

    /// A TEXT longer than the cap is refused before the string is
    /// allocated: no text bytes follow the length, so a read-then-check
    /// would report a truncation instead.
    #[test]
    fn oversized_text_is_refused_before_allocating() {
        let mut payload = Builder::new();
        payload.u32(16);
        payload.unicode("");
        payload.id("null");
        payload.u32(1);
        payload.id("Nm  ");
        payload.raw(b"TEXT");
        payload.u32(u32::try_from(MAX_TEXT_CHARS + 1).expect("the cap fits a u32"));
        let file = abr_file(1, &[(b"desc", payload.into_vec())]);

        assert_eq!(
            read_abr(&file).unwrap_err(),
            AbrError::DescriptorTooLarge {
                what: "text",
                count: (MAX_TEXT_CHARS + 1) as u64,
                max: MAX_TEXT_CHARS as u64,
            }
        );
    }

    /// Lists nested past the depth cap are refused by name.
    #[test]
    fn nesting_past_the_depth_cap_is_refused() {
        let mut deep = Val::List(Vec::new());
        for _ in 0..MAX_DESC_DEPTH {
            deep = Val::List(vec![deep]);
        }
        let file = abr_file(1, &[(b"desc", root_payload(&[("deep", deep)]))]);

        assert_eq!(
            read_abr(&file).unwrap_err(),
            AbrError::DescriptorTooLarge {
                what: "nesting depth",
                count: (MAX_DESC_DEPTH + 1) as u64,
                max: MAX_DESC_DEPTH as u64,
            }
        );
    }

    /// An unknown unit code is refused by name.
    #[test]
    fn unknown_unit_code_is_invalid() {
        let payload = root_payload(&[("Dmtr", Val::Units(b"#Zzz", 1.0))]);
        let file = abr_file(1, &[(b"desc", payload)]);

        assert_eq!(
            read_abr(&file).unwrap_err(),
            AbrError::InvalidDescriptorUnits("#Zzz".to_string())
        );
    }

    /// A desc whose declared 8BIM size runs past EOF is a malformed
    /// DESCRIPTOR under `read_abr`; `read_samples` keeps row 268's plain
    /// `Malformed`.
    #[test]
    fn truncated_desc_is_malformed_descriptor() {
        let mut file = Builder::new();
        file.i16(6);
        file.i16(1);
        file.raw(b"8BIM");
        file.raw(b"desc");
        file.u32(64);
        file.u32(16);
        file.u32(0);
        let file = file.into_vec();

        let error = read_abr(&file).unwrap_err();
        assert!(
            matches!(error, AbrError::MalformedDescriptor { .. }),
            "expected MalformedDescriptor, got {error:?}"
        );
        let error = read_samples(&file).unwrap_err();
        assert!(
            matches!(error, AbrError::Malformed { .. }),
            "expected Malformed, got {error:?}"
        );
    }

    /// A `phry` section is parsed (a bad version is caught) and ignored.
    #[test]
    fn phry_section_parses_and_is_ignored() {
        let phry = root_payload(&[("Nm  ", Val::Text("physics"))]);
        let file = abr_file(1, &[(b"phry", phry), (b"samp", tip_record())]);

        let abr = read_abr(&file).expect("the phry fixture parses");
        assert!(abr.brushes.is_empty());
        assert_eq!(abr.skipped_shape_classes, 0);
        assert_eq!(abr.samples.len(), 1);
        assert_eq!(abr.samples[0].alpha, TIP_ALPHA);

        let mut bad = Builder::new();
        bad.u32(15);
        let bad_file = abr_file(1, &[(b"phry", bad.into_vec())]);
        assert_eq!(
            read_abr(&bad_file).unwrap_err(),
            AbrError::UnsupportedDescriptorVersion(15)
        );
    }

    /// `read_abr` keeps the v8 refusal unchanged.
    #[test]
    fn read_abr_refuses_v8() {
        let mut file = Builder::new();
        file.i16(8);
        file.i16(2);
        file.raw(b"8BIM");
        file.raw(b"desc");
        file.u32(0);

        assert_eq!(
            read_abr(&file.into_vec()).unwrap_err(),
            AbrError::UnsupportedVersion { version: 8 }
        );
    }

    /// The garbage desc `read_samples` skips (pin above) is refused by
    /// the strict full read: the samples-only mode is what keeps it green.
    #[test]
    fn read_abr_parses_the_desc_read_samples_skips() {
        let descriptor = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02];
        let file = abr_file(1, &[(b"desc", descriptor)]);

        assert!(read_samples(&file)
            .expect("samples-only skips desc")
            .is_empty());
        assert_eq!(
            read_abr(&file).unwrap_err(),
            AbrError::UnsupportedDescriptorVersion(0xDEAD_BEEF)
        );
    }

    /// A brush descriptor carrying the texture keys; `pattern_id` is what
    /// `Txtr.Idnt` names.
    fn textured_brush(use_texture: bool, pattern_id: &'static str) -> Val {
        Val::Obj(
            "_",
            vec![
                ("Nm  ", Val::Text("Texture Brush")),
                ("useTexture", Val::Bool(use_texture)),
                ("textureScale", Val::Units(b"#Prc", 66.0)),
                ("textureDepth", Val::Units(b"#Prc", 100.0)),
                ("InvT", Val::Bool(false)),
                ("textureBlendMode", Val::Enum("BlnM", "CBrn")),
                ("TxtC", Val::Bool(true)),
                (
                    "Txtr",
                    Val::Obj(
                        "null",
                        vec![
                            ("Idnt", Val::Text(pattern_id)),
                            ("Nm  ", Val::Text("Bobbys Paint")),
                        ],
                    ),
                ),
                (
                    "Brsh",
                    Val::Obj(
                        "computedBrush",
                        vec![
                            ("Dmtr", Val::Units(b"#Pxl", 40.0)),
                            ("Rndn", Val::Units(b"#Prc", 100.0)),
                            ("Hrdn", Val::Units(b"#Prc", 100.0)),
                        ],
                    ),
                ),
            ],
        )
    }

    /// A minimal 1x1 grayscale `patt` record: one raw sample, then the two
    /// empty mask slots. The body is 94 bytes, so the record carries two
    /// pad bytes.
    fn tiny_pattern_record() -> Vec<u8> {
        let mut body = Builder::new();
        body.u32(1); // record version
        body.u32(1); // grayscale
        body.i16(0); // vertical size (unused)
        body.i16(0); // horizontal size (unused)
        body.u32(3); // name: three UTF-16 units
        for unit in "Pat".encode_utf16() {
            body.u16(unit);
        }
        body.pascal("pid");
        body.u32(3); // pixel-array version
        body.u32(0); // pixel-array length (ignored)
        body.u32(0); // top
        body.u32(0); // left
        body.u32(1); // bottom
        body.u32(1); // right
        body.u32(1); // channel count -> three slots
        body.u32(1); // written
        body.u32(24); // slot length = 23 + one sample
        body.u32(8); // depth
        body.u32(0); // channel top
        body.u32(0); // channel left
        body.u32(1); // channel bottom
        body.u32(1); // channel right
        body.u16(8); // depth again
        body.u8(0); // raw
        body.u8(77); // the sample
        body.u32(0); // empty mask slot
        body.u32(0); // empty mask slot
        let mut body = body.into_vec();
        assert_eq!(body.len(), 94, "the synthetic record body is pinned");
        let declared = u32::try_from(body.len()).expect("test record fits a u32");
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }
        let mut record = Builder::new();
        record.u32(declared);
        record.raw(&body);
        record.into_vec()
    }

    /// Wave 3.1a pin 14: `useTexture` + `Txtr` maps every texture field
    /// exactly, and the rest of the brush is untouched.
    #[test]
    fn parse_brush_maps_the_texture_behind_the_use_texture_gate() {
        let file = abr_file(
            1,
            &[(b"desc", desc_payload(vec![textured_brush(true, "pat-1")]))],
        );
        let abr = read_abr(&file).expect("textured desc parses");
        assert_eq!(abr.brushes.len(), 1);
        let texture = abr.brushes[0]
            .texture
            .as_ref()
            .expect("useTexture + Txtr yields a texture");
        assert_eq!(texture.pattern_id, "pat-1");
        assert_eq!(texture.pattern_name, "Bobbys Paint");
        assert_eq!(texture.scale, 0.66);
        assert_eq!(texture.depth, 1.0);
        assert!(!texture.invert);
        assert_eq!(texture.blend_mode, "BlnM.CBrn");
        assert!(texture.each_tip);
        assert_eq!(abr.brushes[0].name, "Texture Brush");
        assert_eq!(abr.brushes[0].diameter_px, 40.0);
    }

    /// The gate is the `useTexture` bool: a `Txtr` without it is dead data.
    #[test]
    fn a_texture_descriptor_without_the_gate_is_none() {
        let file = abr_file(
            1,
            &[(b"desc", desc_payload(vec![textured_brush(false, "pat-1")]))],
        );
        let abr = read_abr(&file).expect("ungated desc parses");
        assert!(abr.brushes[0].texture.is_none());
    }

    /// Wave 3.1a: the `patt` arm in `walk` is wired — a real record decodes
    /// end to end and the brush's texture resolves to it.
    #[test]
    fn a_pattern_section_decodes_through_the_walk() {
        let file = abr_file(
            1,
            &[
                (b"patt", tiny_pattern_record()),
                (b"desc", desc_payload(vec![textured_brush(true, "pid")])),
            ],
        );
        let abr = read_abr(&file).expect("the pattern fixture parses");
        assert_eq!(abr.patterns.len(), 1);
        assert_eq!(abr.patterns[0].id, "pid");
        assert_eq!(abr.patterns[0].name, "Pat");
        assert_eq!((abr.patterns[0].width, abr.patterns[0].height), (1, 1));
        assert_eq!(abr.patterns[0].rgba, vec![77, 77, 77, 255]);
        assert_eq!(abr.skipped_patterns, 0);
        let pattern = match_pattern(&abr.brushes[0], &abr).expect("the texture resolves");
        assert_eq!(pattern.id, "pid");
    }

    /// Wave 3.1a pin 10 (walk level): a record the decoder refuses costs
    /// the pattern only — samples and a following `desc` still parse.
    ///
    /// One variant stands for all seven (`abr_patt.rs`
    /// `undecodable_records_are_skipped_not_fatal`): the section loop has a
    /// single `Err(_) => skip` arm, so the walk cannot tell them apart.
    #[test]
    fn a_bad_pattern_record_does_not_break_the_file() {
        let mut patt = Builder::new();
        patt.u32(4);
        patt.u32(2); // record version 2 is undecodable
        let file = abr_file(
            1,
            &[
                (b"patt", patt.into_vec()),
                (b"samp", tip_record()),
                (b"desc", desc_payload(vec![sampled_brush("_", "tip1")])),
            ],
        );
        let abr = read_abr(&file).expect("a bad patt record is soft");
        assert!(abr.patterns.is_empty());
        assert_eq!(abr.skipped_patterns, 1);
        assert_eq!(abr.samples.len(), 1);
        assert_eq!(
            abr.brushes,
            vec![expected_sampled(AbrToolClass::Brush, "tip1")]
        );
    }

    /// Wave 3.1a pin 11 (walk level): a record whose declared length runs
    /// past the section end is counted once, and nothing else is disturbed.
    #[test]
    fn a_pattern_record_past_the_section_end_is_soft_and_counted_once() {
        let mut patt = Builder::new();
        patt.u32(1000); // declared length far past the 4-byte payload
        patt.u32(1);
        let file = abr_file(
            1,
            &[
                (b"patt", patt.into_vec()),
                (b"samp", tip_record()),
                (b"desc", desc_payload(vec![sampled_brush("_", "tip1")])),
            ],
        );
        let abr = read_abr(&file).expect("a record past the section is soft");
        assert!(abr.patterns.is_empty());
        assert_eq!(abr.skipped_patterns, 1);
        assert_eq!(abr.samples.len(), 1);
        assert_eq!(abr.brushes.len(), 1);
    }

    /// Wave 3.1a pin 13: `read_samples` skips `patt` by length, hostile or
    /// not, and returns identical samples.
    #[test]
    fn read_samples_ignores_a_garbage_patt_section() {
        let clean = abr_file(1, &[(b"samp", tip_record())]);
        let mut patt = Builder::new();
        patt.u32(0xFFFF_FFFF); // an absurd declared record length
        patt.raw(&[0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03]);
        let dirty = abr_file(1, &[(b"patt", patt.into_vec()), (b"samp", tip_record())]);

        let without = read_samples(&clean).expect("the clean file parses");
        let with = read_samples(&dirty).expect("a garbage patt payload is skipped");
        assert_eq!(without.len(), 1);
        assert_eq!(with.len(), 1);
        assert_eq!(without[0].id, with[0].id);
        assert_eq!(without[0].alpha, with[0].alpha);
        assert_eq!(
            (without[0].width, without[0].height),
            (with[0].width, with[0].height)
        );
        // The full read is soft on `patt` too.
        let abr = read_abr(&dirty).expect("the full read is soft on patt");
        assert!(abr.patterns.is_empty());
        assert_eq!(abr.skipped_patterns, 1);
        assert_eq!(abr.samples.len(), 1);
    }

    /// Wave 3.1a pin 15: `match_pattern` resolves the brush's texture id,
    /// and an unknown id (or no texture) resolves to none.
    #[test]
    fn match_pattern_resolves_the_texture_id() {
        let mut brush = expected_sampled(AbrToolClass::Brush, "tip1");
        brush.texture = Some(AbrTexture {
            pattern_id: "pat-1".to_string(),
            pattern_name: "Pattern".to_string(),
            scale: 1.0,
            depth: 1.0,
            invert: false,
            blend_mode: String::new(),
            each_tip: false,
        });
        let file = AbrFile {
            samples: Vec::new(),
            brushes: vec![brush],
            patterns: vec![AbrPattern {
                id: "pat-1".to_string(),
                name: "Pattern".to_string(),
                width: 1,
                height: 1,
                rgba: vec![9, 8, 7, 255],
            }],
            skipped_shape_classes: 0,
            skipped_patterns: 0,
        };

        let pattern = match_pattern(&file.brushes[0], &file).expect("the id resolves");
        assert_eq!(pattern.name, "Pattern");
        assert_eq!(pattern.rgba, vec![9, 8, 7, 255]);

        let mut unknown = file.brushes[0].clone();
        unknown.texture = Some(AbrTexture {
            pattern_id: "missing".to_string(),
            ..file.brushes[0]
                .texture
                .clone()
                .expect("the fixture texture")
        });
        assert!(match_pattern(&unknown, &file).is_none());

        let mut plain = file.brushes[0].clone();
        plain.texture = None;
        assert!(match_pattern(&plain, &file).is_none());
    }
}
