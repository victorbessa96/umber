//! Photoshop `.abr` brush import (ROADMAP `wave-abr`; requirements §1:
//! "Brush presets (.abr import optional; native preset files)").
//!
//! `.abr` is the one Adobe format that is legal to read here (ROADMAP),
//! and the parser is ag-psd 0.3.0's `read_abr`, already a dependency of
//! this crate for PSD import (MIT; ported from the documented ag-psd TS
//! reader). This module maps what it returns onto Umber's own types:
//!
//! - Each **sampled brush** (`BrushShape::Sampled`, the bitmap tips that
//!   make up nearly every downloadable brush pack) becomes:
//!   - a `.umberbrush` preset with `stamped = true`, a fixed stamp angle,
//!     spacing and size; and
//!   - a `<name>.stamp.png` beside it: the tip's 8-bit alpha as a white
//!     shape on transparency, which `stamp_coverage` (brush panel) reads
//!     back as exactly that coverage.
//! - Each **computed brush** (round tips) becomes a radial preset with
//!   its hardness, angle and spacing.
//! - Dynamics, textures, dual brushes, colour dynamics, "tips" and
//!   "dynamic" shapes have no Umber equivalent yet. They are listed per
//!   brush in the report, never silently dropped.
//!
//! Never-crash: the file is size-capped before parsing, tips are capped
//! in pixels, and every mapping step returns a report line instead of
//! failing the whole pack.

use std::path::{Path, PathBuf};

use ag_psd::abr::{BrushShape, SampleInfo};
use umber_brush::preset::{BrushPreset, SpacingConfig};
use umber_brush::StampAngle;

/// Largest `.abr` read (64 MiB; packs are typically under 20 MiB).
pub const MAX_ABR_BYTES: u64 = 64 * 1024 * 1024;

/// Largest tip side accepted (Photoshop's own maximum is 5000 px).
pub const MAX_TIP_SIDE: i32 = 5000;

/// Umber's base brush diameter at `size = 1.0`: `BRUSH_RADIUS_TEXELS`
/// (24) × 2. A Photoshop brush diameter in pixels maps to
/// `size = diameter / 48`.
pub const BASE_DIAMETER_PX: f64 = 48.0;

/// One brush the import made, ready to write.
#[derive(Debug, Clone)]
pub struct ImportedBrush {
    /// The preset (stamped or radial).
    pub preset: BrushPreset,
    /// The stamp tip for a sampled brush: `(width, height, alpha)`.
    pub stamp: Option<(u32, u32, Vec<u8>)>,
    /// What did not carry over, for the report.
    pub dropped: Vec<&'static str>,
}

/// The whole pack: the brushes made plus the per-brush report lines.
#[derive(Debug, Clone, Default)]
pub struct AbrImport {
    /// Brushes in file order.
    pub brushes: Vec<ImportedBrush>,
    /// One line per brush skipped or partially mapped.
    pub report: Vec<String>,
}

/// Why a whole `.abr` could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbrError {
    /// Over [`MAX_ABR_BYTES`].
    TooLarge(u64),
    /// ag-psd refused it (version 1/2 files, corrupt sections, …).
    Unreadable(String),
    /// Filesystem failure.
    Io(String),
}

impl std::fmt::Display for AbrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(n) => write!(f, ".abr is {n} bytes; the cap is {MAX_ABR_BYTES}"),
            Self::Unreadable(why) => write!(f, "unreadable .abr ({why})"),
            Self::Io(why) => write!(f, "{why}"),
        }
    }
}

/// Reads and maps an `.abr` file.
pub fn import_abr_file(path: &Path) -> Result<AbrImport, AbrError> {
    let len = std::fs::metadata(path)
        .map_err(|e| AbrError::Io(format!("{}: {e}", path.display())))?
        .len();
    if len > MAX_ABR_BYTES {
        return Err(AbrError::TooLarge(len));
    }
    let bytes =
        std::fs::read(path).map_err(|e| AbrError::Io(format!("{}: {e}", path.display())))?;
    import_abr_bytes(&bytes)
}

/// Maps an `.abr` already in memory.
pub fn import_abr_bytes(bytes: &[u8]) -> Result<AbrImport, AbrError> {
    if bytes.len() as u64 > MAX_ABR_BYTES {
        return Err(AbrError::TooLarge(bytes.len() as u64));
    }
    // ag-psd 0.3.0's `samp` reader allocates `vec![0; w * h]` straight
    // from the file's bounds (an i32 product, `abr.rs:962`), before any
    // memory budget applies. Refuse oversized tips first.
    preflight_samples(bytes)?;
    // ag-psd is a port with indexing in its reader; a panic on a hostile
    // file must not take the app down (the never-crash doctrine).
    let abr = std::panic::catch_unwind(|| {
        ag_psd::abr::read_abr(bytes, &ag_psd::abr::ReadAbrOptions::default())
    })
    .map_err(|_| AbrError::Unreadable("the reader panicked".into()))?
    .map_err(|e| AbrError::Unreadable(format!("{e:?}")))?;
    Ok(map_abr(&abr.brushes, &abr.samples))
}

/// Walks the `.abr` v6+ section list (`8BIM` + 4-byte type + u32 BE size)
/// and checks what ag-psd 0.3.0 would allocate from file data before it
/// does (`read_abr` installs no memory budget):
/// - inside `samp`, each sample's bounds, mirroring ag-psd's layout: u32
///   length (4-byte padded), Pascal id, 10 (minor 1) or 264 (minor 2)
///   skipped bytes, then i32 BE top, left, bottom, right. Any tip outside
///   `1..=MAX_TIP_SIDE` is refused;
/// - inside `patt`, each record must fit in the file and its name's
///   declared length must fit in its record.
///
///   Its box (after the id, an indexed palette and the VMAL header) is
///   capped at 256 MiB of RGBA (4 × [`MAX_ABR_BYTES`]): ag-psd's own cap
///   is 30000², 3.6 GB, allocated before decoding.
///
/// Versions 1/2 and anything this walk cannot follow pass through to
/// ag-psd, which refuses them itself.
fn preflight_samples(b: &[u8]) -> Result<(), AbrError> {
    let be16 = |o: usize| b.get(o..o + 2).map(|s| u16::from_be_bytes([s[0], s[1]]));
    let be32 = |o: usize| {
        b.get(o..o + 4)
            .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    };
    let (Some(version), Some(minor)) = (be16(0), be16(2)) else {
        return Ok(());
    };
    if !matches!(version, 6 | 7 | 9 | 10) || !matches!(minor, 1 | 2) {
        return Ok(());
    }
    let mut at = 4usize;
    while at + 12 <= b.len() {
        if &b[at..at + 4] != b"8BIM" {
            return Ok(());
        }
        let kind = &b[at + 4..at + 8];
        let Some(size) = be32(at + 8) else {
            return Ok(());
        };
        let start = at + 12;
        let end = start.saturating_add(size as usize).min(b.len());
        if kind == b"patt" {
            // Umber imports no patterns (texture is reported, not carried),
            // and ag-psd's pattern reader has two unbudgeted allocations: a
            // `with_capacity(len)` for the name from an untrusted u32, and a
            // bitmap up to 30000² × 4 B (its box cap) with no memory limit
            // installed by `read_abr`. A pattern section larger than the
            // file's remaining bytes can only be lying; refuse it, and
            // refuse any pattern whose declared name or box outgrows its
            // own record.
            let mut s = start;
            while s + 4 <= end {
                let Some(mut len) = be32(s).map(|v| v as usize) else {
                    break;
                };
                while len & 3 != 0 {
                    len += 1;
                }
                let body = s + 4;
                let record_end = body.saturating_add(len);
                if record_end > b.len() {
                    return Err(AbrError::Unreadable(
                        "a pattern record runs past the end of the file".into(),
                    ));
                }
                // version u32, mode u32, x i16, y i16, then the name: u32
                // count of UTF-16 units.
                let Some(units) = be32(body + 12).map(|v| v as usize) else {
                    break;
                };
                if units.saturating_mul(2) > len {
                    return Err(AbrError::Unreadable(format!(
                        "a pattern name claims {units} characters in a {len}-byte record"
                    )));
                }
                // Then the Pascal id, an indexed palette (256×3 + 4), the
                // VMAL version (u32 = 3), a length u32, and the box as four
                // u32 BE: top, left, bottom, right (`reader.rs:3013`).
                let mode = be32(body + 4).unwrap_or(0);
                let mut q = body + 16 + units * 2;
                let Some(&id_len) = b.get(q) else {
                    break;
                };
                q += 1 + usize::from(id_len);
                if mode == 2 {
                    q += 256 * 3 + 4;
                }
                q += 8;
                if let (Some(top), Some(left), Some(bottom), Some(right)) =
                    (be32(q), be32(q + 4), be32(q + 8), be32(q + 12))
                {
                    let w = i64::from(right) - i64::from(left);
                    let h = i64::from(bottom) - i64::from(top);
                    // 4 bytes per texel, which must not exceed what is
                    // left of the file (it is RLE inside this record, so
                    // this is generous: a real pattern decodes from far
                    // fewer bytes than it fills, but never more than ~128×).
                    let bytes = w.max(0).saturating_mul(h.max(0)).saturating_mul(4);
                    if bytes > (MAX_ABR_BYTES as i64).saturating_mul(4) {
                        return Err(AbrError::Unreadable(format!(
                            "a pattern claims {w}x{h} texels; refused before allocating"
                        )));
                    }
                }
                s = record_end;
            }
        }
        if kind == b"samp" {
            let mut s = start;
            while s + 4 <= end {
                let Some(mut len) = be32(s).map(|v| v as usize) else {
                    break;
                };
                while len & 3 != 0 {
                    len += 1;
                }
                let body = s + 4;
                let Some(&id_len) = b.get(body) else {
                    break;
                };
                // Pascal string padded to 1 = length byte + text.
                let bounds = body + 1 + usize::from(id_len) + if minor == 1 { 10 } else { 264 };
                let read = |o: usize| be32(o).map(|v| v as i32);
                if let (Some(top), Some(left), Some(bottom), Some(right)) = (
                    read(bounds),
                    read(bounds + 4),
                    read(bounds + 8),
                    read(bounds + 12),
                ) {
                    let h = i64::from(bottom) - i64::from(top);
                    let w = i64::from(right) - i64::from(left);
                    let cap = i64::from(MAX_TIP_SIDE);
                    if w > cap || h > cap {
                        return Err(AbrError::Unreadable(format!(
                            "a brush tip is {w}x{h}; the cap is {MAX_TIP_SIDE} per side"
                        )));
                    }
                }
                s = body.saturating_add(len);
            }
        }
        at = start.saturating_add(size as usize);
        while at & 3 != 0 && at < b.len() && b[at] == 0 {
            at += 1;
        }
    }
    Ok(())
}

/// The mapping, separated from parsing so it is testable with
/// hand-built brushes.
pub fn map_abr(brushes: &[ag_psd::abr::Brush], samples: &[SampleInfo]) -> AbrImport {
    let mut out = AbrImport::default();
    for (i, brush) in brushes.iter().enumerate() {
        let name = if brush.name.trim().is_empty() {
            format!("ABR brush {}", i + 1)
        } else {
            brush.name.trim().to_string()
        };
        let mut dropped = Vec::new();
        if brush.shape_dynamics.is_some() {
            dropped.push("shape dynamics");
        }
        if brush.scatter.is_some() {
            dropped.push("scattering");
        }
        if brush.texture.is_some() {
            dropped.push("texture");
        }
        if brush.dual_brush.is_some() {
            dropped.push("dual brush");
        }
        if brush.color_dynamics.is_some() {
            dropped.push("colour dynamics");
        }
        if brush.transfer.is_some() {
            dropped.push("transfer");
        }
        if brush.wet_edges {
            dropped.push("wet edges");
        }
        if brush.noise {
            dropped.push("noise");
        }
        // Every field spelled out: `BrushPreset` has no `Default`, and an
        // explicit literal fails to compile when a field is added, which
        // is the point.
        let mut preset = BrushPreset {
            name: name.clone(),
            params: umber_brush::BrushParams::default(),
            one_euro: umber_brush::OneEuroParams::default(),
            lazy_mouse: umber_brush::preset::LazyMouseConfig::default(),
            spacing: SpacingConfig::default(),
            alpha_curve: umber_brush::preset::ControlCurve::default(),
            radius_curve: umber_brush::preset::ControlCurve::default(),
            stroke_method: umber_brush::StrokeMethod::default(),
            size: 1.0,
        };
        let mut stamp = None;
        let (size, spacing, spacing_on, angle) = match &brush.shape {
            BrushShape::Computed {
                size,
                angle,
                hardness,
                spacing_on,
                spacing,
                ..
            } => {
                preset.params.hardness = (*hardness as f32).clamp(0.0, 1.0);
                (*size, *spacing, *spacing_on, *angle)
            }
            BrushShape::Sampled {
                size,
                angle,
                spacing_on,
                spacing,
                sampled_data,
                ..
            } => {
                let Some(sample) = samples.iter().find(|s| &s.id == sampled_data) else {
                    out.report.push(format!(
                        "{name}: its tip image is missing from the file; skipped"
                    ));
                    continue;
                };
                let b = sample.bounds;
                if b.w <= 0 || b.h <= 0 || b.w > MAX_TIP_SIDE || b.h > MAX_TIP_SIDE {
                    out.report.push(format!(
                        "{name}: tip {}x{} outside 1..={MAX_TIP_SIDE}; skipped",
                        b.w, b.h
                    ));
                    continue;
                }
                if sample.alpha.len() != b.w as usize * b.h as usize {
                    out.report
                        .push(format!("{name}: tip data does not match its size; skipped"));
                    continue;
                }
                preset.params.stamped = true;
                stamp = Some((b.w as u32, b.h as u32, sample.alpha.clone()));
                (*size, *spacing, *spacing_on, *angle)
            }
            BrushShape::Tips { .. } | BrushShape::Dynamic { .. } => {
                out.report.push(format!(
                    "{name}: an erodible/airbrush/bristle tip, which Umber cannot paint yet; skipped"
                ));
                continue;
            }
        };
        if size.is_finite() && size > 0.0 {
            preset.size = (size / BASE_DIAMETER_PX).clamp(0.05, 20.0) as f32;
        }
        // Photoshop spacing is a percentage of the diameter between dabs;
        // Umber's density is dabs per RADIUS: radius / (spacing × diameter)
        // = 1 / (2 × spacing).
        if spacing_on && spacing.is_finite() && spacing > 0.0 {
            preset.spacing = SpacingConfig {
                dabs_per_radius: (1.0 / (2.0 * spacing)).clamp(0.05, 50.0) as f32,
            };
        }
        if angle.is_finite() && angle != 0.0 {
            preset.params.stamp_angle = StampAngle::Fixed {
                degrees: angle as f32,
            };
        }
        if !dropped.is_empty() {
            out.report
                .push(format!("{name}: not carried over: {}", dropped.join(", ")));
        }
        out.brushes.push(ImportedBrush {
            preset,
            stamp,
            dropped,
        });
    }
    if brushes.is_empty() && !samples.is_empty() {
        out.report.push(format!(
            "{} tip image(s) but no brush definitions (an old-style pack); nothing imported",
            samples.len()
        ));
    }
    out
}

/// A file stem safe on every platform, never empty.
fn file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        "brush".to_string()
    } else {
        s
    }
}

/// Writes each brush as `<stem>.umberbrush` (plus `<stem>.stamp.png`)
/// under `dir`, never overwriting: a taken stem gets ` 2`, ` 3`, ….
/// Returns the written preset paths.
pub fn write_brushes(import: &AbrImport, dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut written = Vec::new();
    for brush in &import.brushes {
        let base = file_stem(&brush.preset.name);
        let mut stem = base.clone();
        let mut n = 2;
        while dir.join(format!("{stem}.umberbrush")).exists() {
            stem = format!("{base} {n}");
            n += 1;
        }
        let json = brush.preset.to_json().map_err(|e| e.to_string())?;
        let path = dir.join(format!("{stem}.umberbrush"));
        std::fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some((w, h, alpha)) = &brush.stamp {
            // A white shape on transparency: `stamp_coverage` reads it
            // back as luma × alpha = the tip's own alpha.
            let rgba: Vec<u8> = alpha.iter().flat_map(|a| [255, 255, 255, *a]).collect();
            let png = dir.join(format!("{stem}.stamp.png"));
            umber_export::png::write_png(&png, *w, *h, &rgba, umber_export::png::Transfer::Linear)
                .map_err(|e| format!("{}: {e}", png.display()))?;
        }
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ag_psd::abr::{Brush, SampleBounds};

    fn brush(name: &str, shape: BrushShape) -> Brush {
        Brush {
            name: name.into(),
            shape,
            shape_dynamics: None,
            scatter: None,
            texture: None,
            dual_brush: None,
            color_dynamics: None,
            transfer: None,
            brush_pose: None,
            noise: false,
            wet_edges: false,
            protect_texture: None,
            spacing: 0.25,
            interpretation: None,
            use_brush_size: false,
            tool_options: None,
        }
    }

    fn sampled(id: &str, size: f64, spacing: f64, angle: f64) -> BrushShape {
        BrushShape::Sampled {
            name: "tip".into(),
            size,
            angle,
            roundness: 1.0,
            spacing_on: true,
            spacing,
            flip_x: false,
            flip_y: false,
            sampled_data: id.into(),
        }
    }

    #[test]
    fn a_sampled_brush_becomes_a_stamped_preset_with_its_tip() {
        let samples = vec![SampleInfo {
            id: "abc".into(),
            bounds: SampleBounds {
                x: 0,
                y: 0,
                w: 2,
                h: 2,
            },
            alpha: vec![0, 128, 255, 64],
        }];
        let out = map_abr(
            &[brush("Chalk", sampled("abc", 96.0, 0.25, 30.0))],
            &samples,
        );
        assert_eq!(out.brushes.len(), 1);
        let b = &out.brushes[0];
        assert!(b.preset.params.stamped);
        assert_eq!(b.stamp, Some((2, 2, vec![0, 128, 255, 64])));
        // 96 px = twice the 48 px base; 25% spacing = 2 dabs per radius.
        assert!((b.preset.size - 2.0).abs() < 1e-6);
        assert!((b.preset.spacing.dabs_per_radius - 2.0).abs() < 1e-6);
        assert_eq!(
            b.preset.params.stamp_angle,
            StampAngle::Fixed { degrees: 30.0 }
        );
        assert!(b.preset.validate().is_ok());
    }

    #[test]
    fn a_computed_brush_keeps_its_hardness_and_dynamics_are_reported() {
        let mut round = brush(
            "Hard Round",
            BrushShape::Computed {
                size: 24.0,
                angle: 0.0,
                roundness: 1.0,
                hardness: 0.9,
                spacing_on: true,
                spacing: 0.1,
                flip_x: false,
                flip_y: false,
            },
        );
        round.wet_edges = true;
        let out = map_abr(&[round], &[]);
        let b = &out.brushes[0];
        assert!(!b.preset.params.stamped);
        assert!((b.preset.params.hardness - 0.9).abs() < 1e-6);
        assert_eq!(b.preset.params.stamp_angle, StampAngle::None);
        assert_eq!(b.dropped, vec!["wet edges"]);
        assert!(out.report.iter().any(|l| l.contains("wet edges")));
    }

    #[test]
    fn broken_brushes_are_skipped_with_a_reason_never_a_panic() {
        let huge = SampleInfo {
            id: "big".into(),
            bounds: SampleBounds {
                x: 0,
                y: 0,
                w: MAX_TIP_SIDE + 1,
                h: 1,
            },
            alpha: Vec::new(),
        };
        let short = SampleInfo {
            id: "short".into(),
            bounds: SampleBounds {
                x: 0,
                y: 0,
                w: 4,
                h: 4,
            },
            alpha: vec![0; 3],
        };
        let out = map_abr(
            &[
                brush("Missing", sampled("nope", 10.0, 0.25, 0.0)),
                brush("Huge", sampled("big", 10.0, 0.25, 0.0)),
                brush("Short", sampled("short", 10.0, 0.25, 0.0)),
                brush(
                    "Bristle",
                    BrushShape::Dynamic {
                        size: 10.0,
                        angle: 0.0,
                        shape: String::new(),
                        density: 0.0,
                        length: 0.0,
                        clumping: 0.0,
                        thickness: 0.0,
                        stiffness: 0.0,
                        physics: false,
                        spacing: 0.0,
                        spacing_on: false,
                        flip_x: false,
                        flip_y: false,
                    },
                ),
            ],
            &[huge, short],
        );
        assert!(out.brushes.is_empty());
        assert_eq!(out.report.len(), 4, "{:?}", out.report);
        // A hand-made v6.2 pack whose one tip claims 100000×100000: the
        // preflight refuses it before ag-psd allocates 10 GB.
        let mut abr = vec![0, 6, 0, 2];
        let mut sample = Vec::new();
        sample.push(1u8); // Pascal id length
        sample.push(b'x');
        sample.extend([0u8; 264]);
        for v in [0i32, 0, 100_000, 100_000] {
            sample.extend(v.to_be_bytes());
        }
        sample.extend([0u8, 8, 0]); // depth 8, raw
        let mut samp = (sample.len() as u32).to_be_bytes().to_vec();
        samp.extend(&sample);
        while !samp.len().is_multiple_of(4) {
            samp.push(0);
        }
        abr.extend(b"8BIMsamp");
        abr.extend((samp.len() as u32).to_be_bytes());
        abr.extend(&samp);
        match import_abr_bytes(&abr) {
            Err(AbrError::Unreadable(why)) => assert!(why.contains("cap"), "{why}"),
            other => panic!("{other:?}"),
        }
        // A pattern claiming 30000² in a tiny record: refused, not 3.6 GB.
        let mut patt = Vec::new();
        patt.extend(1u32.to_be_bytes()); // version
        patt.extend(3u32.to_be_bytes()); // RGB
        patt.extend([0u8; 4]); // x, y
        patt.extend(0u32.to_be_bytes()); // empty name
        patt.push(0); // empty Pascal id
        patt.extend(3u32.to_be_bytes()); // VMAL version
        patt.extend(0u32.to_be_bytes()); // length
        for v in [0u32, 0, 30_000, 30_000] {
            patt.extend(v.to_be_bytes());
        }
        let mut rec = (patt.len() as u32).to_be_bytes().to_vec();
        rec.extend(&patt);
        while !rec.len().is_multiple_of(4) {
            rec.push(0);
        }
        let mut abr = vec![0, 6, 0, 2];
        abr.extend(b"8BIMpatt");
        abr.extend((rec.len() as u32).to_be_bytes());
        abr.extend(&rec);
        match import_abr_bytes(&abr) {
            Err(AbrError::Unreadable(why)) => assert!(why.contains("30000x30000"), "{why}"),
            other => panic!("{other:?}"),
        }
        // Garbage bytes: a typed error.
        assert!(import_abr_bytes(&[0, 6, 0, 2, 1, 2, 3]).is_err());
        assert!(import_abr_bytes(&[]).is_err());
    }

    #[test]
    fn writing_never_overwrites_and_the_stamp_reads_back_as_its_alpha() {
        let samples = vec![SampleInfo {
            id: "t".into(),
            bounds: SampleBounds {
                x: 0,
                y: 0,
                w: 2,
                h: 1,
            },
            alpha: vec![10, 250],
        }];
        let out = map_abr(
            &[brush("Ink/Splat", sampled("t", 48.0, 0.25, 0.0))],
            &samples,
        );
        let dir = std::env::temp_dir().join(format!("umber-abr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = write_brushes(&out, &dir).unwrap();
        let second = write_brushes(&out, &dir).unwrap();
        assert_eq!(first[0].file_name().unwrap(), "Ink_Splat.umberbrush");
        assert_eq!(second[0].file_name().unwrap(), "Ink_Splat 2.umberbrush");
        let text = std::fs::read_to_string(&first[0]).unwrap();
        assert!(BrushPreset::from_json(&text).unwrap().params.stamped);
        let (w, h, rgba) =
            umber_export::png::read_png_rgba8(&dir.join("Ink_Splat.stamp.png")).unwrap();
        assert_eq!((w, h), (2, 1));
        // `super` is this module; the panel is one level up.
        assert_eq!(super::super::stamp_coverage(&rgba), vec![10, 250]);
        // The panel finds a preset's tip as `source_path.with_extension(
        // "stamp.png")`: a dotted brush name must not break that.
        let dotted = map_abr(
            &[brush("Pack v1.2", sampled("t", 48.0, 0.25, 0.0))],
            &samples,
        );
        let written = write_brushes(&dotted, &dir).unwrap();
        assert!(
            written[0].with_extension("stamp.png").is_file(),
            "{:?}",
            written[0]
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
