//! `.abr` → `.umberbrush` mapping (row 283 of the wave-abr design brief,
//! `docs/loop/briefs/2026-11-07-wave-abr-design.md` — the target-shape
//! table is this module's contract).
//!
//! [`import_abr`] reads a file with [`read_abr`] and maps every
//! [`AbrBrush`] to an [`AbrImportEntry`]: a [`BrushPreset`] ready for
//! `to_json` + `validate()`, plus, for sampled brushes, the stamp bitmap
//! the `<stem>.stamp.png` sidecar convention (`preset.rs`) wants. Pure:
//! no I/O, no GPU, no egui.
//!
//! Mapping:
//!
//! * `name` ← the brush name (empty → `"abr brush <index>"`);
//! * `lazy_mouse.radius_px` ← `diameter_px / 2`, clamped to
//!   `1.0..=4096.0`; non-finite → 1.0;
//! * `params.hardness` ← `hardness` clamped to `0..=1` (non-finite → 1);
//! * spacing: off → `dabs_per_radius = 100` (dense, smooth); on →
//!   `1 / (2 * spacing)` clamped to `1..=100` (25% spacing = a dab every
//!   0.25 diameters = 2 dabs per radius). `spacing == 0` / non-finite
//!   take the dense end;
//! * `params.stamp_angle` ← `Fixed { degrees: angle_deg }` (non-finite → 0);
//! * a sampled brush whose tip is found by [`match_sample`] gets
//!   `params.stamped = true` and the stamp `(w, h, RGBA8)` with RGB =
//!   white and A = the tip's alpha byte (the same white-plus-coverage
//!   convention as the `.svg` stencil raster: white is multiplied by the
//!   stroke colour, so the tip tints like a paint);
//! * a brush with a texture (`useTexture` + `Txtr`) whose pattern id a
//!   `patt` record carries gets `texture` = the index of that pattern in
//!   [`AbrImport::patterns`] plus its parameters; the pattern itself is
//!   carried once per id, however many brushes share it;
//! * any other brush is a radial preset (`stamped = false`, no stamp).
//!
//! Judgment calls: a duplicate stamp (byte-equal to an earlier emitted
//! entry's) drops the LATER entry and counts it — two brushes sharing a
//! tip add no shape variety, and the first keeps its settings; the
//! duplicate rule still compares stamps only, so a dropped duplicate's
//! texture is lost (revisiting that is kill 3.1c's call) and its pattern
//! stays carried because the source file references it; an orphan sample
//! (no brush references its id) is counted, never an error; a zero-area
//! tip is treated as missing; a textured brush whose pattern record is
//! missing keeps its other settings, gets `texture: None` and is counted;
//! an unreferenced pattern record (or a second record with an already
//! carried id) is counted and not carried.
//!
//! The decoded pattern is cloned into [`AbrImport::patterns`]: the CLI
//! consumes the import after the source bytes are gone, so the mapping
//! cannot borrow from [`AbrFile`]. One clone per referenced pattern.
//!
//! Non-goals (v1): v8+ files, dual brush, scattering, roundness, flips,
//! dynamics → engine wiring (the parsed [`AbrBrush::dynamics`] is dropped),
//! sampling the carried texture in the paint engine (kill 3.1c — the
//! import writes no pixels that use it), `.abr` writing (never), and the
//! CLI fan-out over a whole file (row 284).

use crate::abr::{match_pattern, match_sample, read_abr, AbrBrush, AbrError, AbrFile, AbrPattern};
use crate::preset::{BrushPreset, ControlCurve, LazyMouseConfig, SpacingConfig};
use crate::{BrushParams, OneEuroParams, StampAngle, StrokeMethod};

/// One imported brush.
#[derive(Debug, Clone, PartialEq)]
pub struct AbrImportEntry {
    /// The mapped preset (passes `validate()`).
    pub preset: BrushPreset,
    /// `(width, height, RGBA8)` — `Some` for sampled brushes. RGB is
    /// white; A is the tip coverage.
    pub stamp: Option<(u32, u32, Vec<u8>)>,
    /// The brush's resolved texture link, `Some` only when the brush names
    /// a pattern the file carries. `pattern_index` indexes
    /// [`AbrImport::patterns`].
    pub texture: Option<AbrImportTexture>,
    /// Index of the source brush in [`AbrFile::brushes`].
    pub brush_index: usize,
}

/// A brush's texture parameters, resolved against [`AbrImport::patterns`].
#[derive(Debug, Clone, PartialEq)]
pub struct AbrImportTexture {
    /// Index into [`AbrImport::patterns`].
    pub pattern_index: usize,
    /// Scale as a fraction (`textureScale`, percent / 100; missing → 1.0).
    pub scale: f64,
    /// Depth as a fraction (`textureDepth`, percent / 100; missing → 1.0).
    pub depth: f64,
    /// Whether the pattern is inverted (`InvT`).
    pub invert: bool,
    /// The stored blend-mode enum string (`BlnM.<code>`), undecoded.
    pub blend_mode: String,
    /// Whether the texture applies to each tip (`TxtC`).
    pub each_tip: bool,
}

/// The result of importing one `.abr` file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AbrImport {
    /// One entry per brush kept.
    pub entries: Vec<AbrImportEntry>,
    /// The patterns at least one brush in the file references, in file
    /// order, one entry per id. Cloned from [`AbrFile::patterns`].
    pub patterns: Vec<AbrPattern>,
    /// `samp` records no brush references.
    pub orphan_samples: u32,
    /// `patt` records no brush references, plus a second record with an
    /// id already carried (the first record wins, the same rule
    /// [`match_pattern`] uses).
    pub orphan_patterns: u32,
    /// Brushes emitted as radial presets (computed brushes, and sampled
    /// brushes whose tip is missing).
    pub brushes_without_sample: u32,
    /// Entries dropped because an earlier entry has the identical stamp.
    pub duplicate_stamps_skipped: u32,
    /// Textured brushes whose pattern id no `patt` record carries; their
    /// `texture` is `None`.
    pub textures_missing_pattern: u32,
}

/// Parses `bytes` as an `.abr` file and maps it (see the module docs).
///
/// # Errors
///
/// Every [`read_abr`] error.
pub fn import_abr(bytes: &[u8]) -> Result<AbrImport, AbrError> {
    Ok(map_file(&read_abr(bytes)?))
}

/// The pure mapping over an already-parsed file.
fn map_file(file: &AbrFile) -> AbrImport {
    let orphan_samples = file
        .samples
        .iter()
        .filter(|sample| {
            !file
                .brushes
                .iter()
                .any(|b| b.sampled_data_id.as_deref() == Some(sample.id.as_str()))
        })
        .count() as u32;
    let mut out = AbrImport {
        orphan_samples,
        ..AbrImport::default()
    };

    // Carry the patterns a brush in this file references: file order, one
    // entry per id (the same first-wins rule `match_pattern` applies). An
    // unreferenced record, or a duplicate id, is counted and dropped.
    for pattern in &file.patterns {
        let referenced = file.brushes.iter().any(|brush| {
            brush
                .texture
                .as_ref()
                .is_some_and(|texture| texture.pattern_id == pattern.id)
        });
        let already_carried = out.patterns.iter().any(|kept| kept.id == pattern.id);
        if referenced && !already_carried {
            out.patterns.push(pattern.clone());
        } else {
            out.orphan_patterns += 1;
        }
    }

    for (index, brush) in file.brushes.iter().enumerate() {
        let stamp = match_sample(brush, file)
            .filter(|s| {
                s.width > 0
                    && s.height > 0
                    && s.alpha.len() as u64 == s.width as u64 * s.height as u64
            })
            .map(|s| {
                let mut rgba = Vec::with_capacity(s.alpha.len() * 4);
                for &a in &s.alpha {
                    rgba.extend_from_slice(&[255, 255, 255, a]);
                }
                (s.width, s.height, rgba)
            });

        if let Some(stamp) = &stamp {
            if out.entries.iter().any(|e| e.stamp.as_ref() == Some(stamp)) {
                out.duplicate_stamps_skipped += 1;
                continue;
            }
        } else {
            out.brushes_without_sample += 1;
        }

        // Resolve the texture against the patterns carried above, through
        // the reader's own link rule. A brush whose id no record carries
        // keeps its other settings and is counted.
        let texture = brush.texture.as_ref().and_then(|texture| {
            let pattern = match_pattern(brush, file)?;
            let pattern_index = out.patterns.iter().position(|kept| kept.id == pattern.id)?;
            Some(AbrImportTexture {
                pattern_index,
                scale: texture.scale,
                depth: texture.depth,
                invert: texture.invert,
                blend_mode: texture.blend_mode.clone(),
                each_tip: texture.each_tip,
            })
        });
        if brush.texture.is_some() && texture.is_none() {
            out.textures_missing_pattern += 1;
        }

        out.entries.push(AbrImportEntry {
            preset: map_preset(brush, index, stamp.is_some()),
            stamp,
            texture,
            brush_index: index,
        });
    }
    out
}

fn map_preset(brush: &AbrBrush, index: usize, stamped: bool) -> BrushPreset {
    let name = if brush.name.trim().is_empty() {
        format!("abr brush {index}")
    } else {
        brush.name.clone()
    };

    let radius = if brush.diameter_px.is_finite() {
        (brush.diameter_px / 2.0).clamp(1.0, 4096.0)
    } else {
        1.0
    };
    let hardness = if brush.hardness.is_finite() {
        brush.hardness.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let dabs = if !brush.spacing_on {
        100.0
    } else {
        let denom = 2.0 * brush.spacing;
        if denom.is_finite() && denom > 0.0 {
            (1.0 / denom).clamp(1.0, 100.0)
        } else {
            100.0
        }
    };
    let degrees = if brush.angle_deg.is_finite() {
        brush.angle_deg as f32
    } else {
        0.0
    };

    BrushPreset {
        name,
        params: BrushParams {
            hardness: hardness as f32,
            stamped,
            stamp_angle: StampAngle::Fixed { degrees },
            ..BrushParams::default()
        },
        one_euro: OneEuroParams::default(),
        lazy_mouse: LazyMouseConfig {
            radius_px: radius as f32,
            ..LazyMouseConfig::default()
        },
        spacing: SpacingConfig {
            dabs_per_radius: dabs as f32,
        },
        alpha_curve: ControlCurve::default(),
        radius_curve: ControlCurve::default(),
        stroke_method: StrokeMethod::default(),
        size: 1.0,
        catalog: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abr::{AbrSample, AbrTexture, AbrToolClass};

    fn brush(name: &str, tip: Option<&str>) -> AbrBrush {
        AbrBrush {
            tool_class: AbrToolClass::Brush,
            name: name.to_owned(),
            diameter_px: 32.0,
            angle_deg: 15.0,
            roundness: 1.0,
            spacing_on: true,
            spacing: 0.25,
            flip_x: false,
            flip_y: false,
            hardness: 0.8,
            sampled_data_id: tip.map(str::to_owned),
            dynamics: None,
            texture: None,
        }
    }

    fn texture(id: &str) -> AbrTexture {
        AbrTexture {
            pattern_id: id.to_owned(),
            pattern_name: "Pattern".to_owned(),
            scale: 0.66,
            depth: 1.0,
            invert: false,
            blend_mode: "BlnM.CBrn".to_owned(),
            each_tip: true,
        }
    }

    fn pattern(id: &str) -> AbrPattern {
        AbrPattern {
            id: id.to_owned(),
            name: "Pattern".to_owned(),
            width: 1,
            height: 1,
            rgba: vec![9, 8, 7, 255],
        }
    }

    fn ramp() -> Vec<u8> {
        (0..64u8).map(|i| i * 4).collect()
    }

    fn sample(id: &str, alpha: Vec<u8>) -> AbrSample {
        AbrSample {
            id: id.to_owned(),
            x: 0,
            y: 0,
            width: 8,
            height: 8,
            alpha,
        }
    }

    fn file(samples: Vec<AbrSample>, brushes: Vec<AbrBrush>) -> AbrFile {
        file_with_patterns(samples, brushes, Vec::new())
    }

    fn file_with_patterns(
        samples: Vec<AbrSample>,
        brushes: Vec<AbrBrush>,
        patterns: Vec<AbrPattern>,
    ) -> AbrFile {
        AbrFile {
            samples,
            brushes,
            patterns,
            skipped_shape_classes: 0,
            skipped_patterns: 0,
        }
    }

    #[test]
    fn sampled_brush_maps_every_field() {
        let out = map_file(&file(
            vec![sample("t1", ramp())],
            vec![brush("Chalk", Some("t1"))],
        ));
        assert_eq!(out.entries.len(), 1);
        assert_eq!(out.orphan_samples, 0);
        assert_eq!(out.brushes_without_sample, 0);
        let e = &out.entries[0];
        assert_eq!(e.brush_index, 0);
        let p = &e.preset;
        assert_eq!(p.name, "Chalk");
        assert_eq!(p.lazy_mouse.radius_px, 16.0);
        assert!((p.params.hardness - 0.8).abs() < 1e-6);
        assert_eq!(p.spacing.dabs_per_radius, 2.0);
        assert_eq!(p.params.stamp_angle, StampAngle::Fixed { degrees: 15.0 });
        assert!(p.params.stamped);
        assert!(p.validate().is_ok());

        let (w, h, rgba) = e.stamp.as_ref().expect("sampled brush has a stamp");
        assert_eq!((*w, *h), (8, 8));
        assert_eq!(rgba.len(), 8 * 8 * 4);
        for (i, px) in rgba.chunks_exact(4).enumerate() {
            assert_eq!(&px[..3], &[255, 255, 255]);
            assert_eq!(px[3], (i as u8) * 4);
        }
    }

    #[test]
    fn spacing_off_is_dense() {
        let mut b = brush("A", None);
        b.spacing_on = false;
        let out = map_file(&file(vec![], vec![b]));
        assert_eq!(out.entries[0].preset.spacing.dabs_per_radius, 100.0);
    }

    #[test]
    fn spacing_zero_and_huge_clamp() {
        let mut b = brush("A", None);
        b.spacing = 0.0;
        let mut c = brush("B", None);
        c.spacing = 10.0; // 1 / 20 -> clamped up to 1
        let out = map_file(&file(vec![], vec![b, c]));
        assert_eq!(out.entries[0].preset.spacing.dabs_per_radius, 100.0);
        assert_eq!(out.entries[1].preset.spacing.dabs_per_radius, 1.0);
    }

    #[test]
    fn degenerate_diameter_clamps_and_validates() {
        for d in [0.0, f64::NAN, -5.0, f64::INFINITY] {
            let mut b = brush("A", None);
            b.diameter_px = d;
            b.hardness = f64::NAN;
            b.angle_deg = f64::NAN;
            let out = map_file(&file(vec![], vec![b]));
            let p = &out.entries[0].preset;
            assert_eq!(p.lazy_mouse.radius_px, 1.0, "diameter {d}");
            assert_eq!(p.params.hardness, 1.0);
            assert_eq!(p.params.stamp_angle, StampAngle::Fixed { degrees: 0.0 });
            assert!(p.validate().is_ok());
        }
    }

    #[test]
    fn computed_brush_is_radial_and_counted() {
        let out = map_file(&file(vec![], vec![brush("Round", None)]));
        let e = &out.entries[0];
        assert!(!e.preset.params.stamped);
        assert!(e.stamp.is_none());
        assert_eq!(out.brushes_without_sample, 1);
        assert!(e.preset.validate().is_ok());
    }

    #[test]
    fn unmatched_sampled_id_is_radial_and_counted() {
        let out = map_file(&file(
            vec![sample("other", ramp())],
            vec![brush("Lost", Some("missing"))],
        ));
        assert_eq!(out.entries.len(), 1);
        assert!(!out.entries[0].preset.params.stamped);
        assert!(out.entries[0].stamp.is_none());
        assert_eq!(out.brushes_without_sample, 1);
        assert_eq!(out.orphan_samples, 1);
    }

    #[test]
    fn shared_tip_second_brush_is_a_duplicate() {
        let out = map_file(&file(
            vec![sample("t1", ramp())],
            vec![brush("First", Some("t1")), brush("Second", Some("t1"))],
        ));
        assert_eq!(out.entries.len(), 1);
        assert_eq!(out.entries[0].preset.name, "First");
        assert_eq!(out.duplicate_stamps_skipped, 1);
    }

    #[test]
    fn empty_name_gets_an_indexed_fallback() {
        let out = map_file(&file(vec![], vec![brush("x", None), brush("", None)]));
        assert_eq!(out.entries[1].preset.name, "abr brush 1");
    }

    #[test]
    fn presets_round_trip_through_json() {
        let out = map_file(&file(
            vec![sample("t1", ramp())],
            vec![brush("Chalk", Some("t1")), brush("Round", None)],
        ));
        for e in &out.entries {
            let json = e.preset.to_json().expect("serializes");
            let back = BrushPreset::from_json(&json).expect("parses");
            assert!(back.validate().is_ok());
            assert_eq!(back, e.preset);
        }
    }

    /// Minimal v6 minor-1 file with one raw 8-bit `samp` record and no
    /// `desc`: parses, yields no entries, and the tip is an orphan.
    #[test]
    fn bytes_with_only_a_sample_report_an_orphan() {
        let mut body = Vec::new();
        let name = b"tip";
        body.push(name.len() as u8);
        body.extend_from_slice(name);
        for v in [0u16; 5] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        for v in [0i32, 0, 2, 2] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        body.extend_from_slice(&8i16.to_be_bytes());
        body.push(0); // raw
        body.extend_from_slice(&[1, 2, 3, 4]);
        let declared = body.len() as u32;
        while body.len() % 4 != 0 {
            body.push(0);
        }
        let mut payload = declared.to_be_bytes().to_vec();
        payload.extend_from_slice(&body);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&6i16.to_be_bytes());
        bytes.extend_from_slice(&1i16.to_be_bytes());
        bytes.extend_from_slice(b"8BIMsamp");
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&payload);

        let out = import_abr(&bytes).expect("parses");
        assert!(out.entries.is_empty());
        assert_eq!(out.orphan_samples, 1);
        assert!(out.patterns.is_empty());
    }

    /// Wave 3.1a pin 16: a textured brush points at the right pattern
    /// index, and the pattern's parameters ride on the entry.
    #[test]
    fn textured_brush_points_at_its_pattern() {
        let mut b = brush("Tex", None);
        b.texture = Some(texture("pat-1"));
        let out = map_file(&file_with_patterns(vec![], vec![b], vec![pattern("pat-1")]));
        assert_eq!(out.patterns.len(), 1);
        assert_eq!(out.patterns[0].id, "pat-1");
        assert_eq!(out.orphan_patterns, 0);
        assert_eq!(out.textures_missing_pattern, 0);
        let t = out.entries[0]
            .texture
            .as_ref()
            .expect("the pattern resolves");
        assert_eq!(t.pattern_index, 0);
        assert_eq!(t.scale, 0.66);
        assert_eq!(t.depth, 1.0);
        assert!(!t.invert);
        assert_eq!(t.blend_mode, "BlnM.CBrn");
        assert!(t.each_tip);
    }

    /// Two brushes sharing one pattern id yield ONE pattern entry, and both
    /// entries point at it. (Computed brushes, so the duplicate-stamp rule
    /// cannot collapse them.)
    #[test]
    fn two_brushes_sharing_a_pattern_yield_one_pattern() {
        let mut first = brush("One", None);
        first.texture = Some(texture("pat-1"));
        let mut second = brush("Two", None);
        second.texture = Some(texture("pat-1"));
        second.texture.as_mut().expect("texture just set").scale = 0.5;
        let out = map_file(&file_with_patterns(
            vec![],
            vec![first, second],
            vec![pattern("pat-1"), pattern("pat-1")],
        ));
        assert_eq!(out.patterns.len(), 1, "one entry per id");
        assert_eq!(out.orphan_patterns, 1, "the duplicate record is counted");
        assert_eq!(out.entries.len(), 2);
        assert_eq!(
            out.entries[0]
                .texture
                .as_ref()
                .expect("resolved")
                .pattern_index,
            0
        );
        assert_eq!(
            out.entries[1]
                .texture
                .as_ref()
                .expect("resolved")
                .pattern_index,
            0
        );
        assert_eq!(
            out.entries[1].texture.as_ref().expect("resolved").scale,
            0.5
        );
    }

    /// An unreferenced pattern is an orphan; a textured brush whose id no
    /// record carries keeps its other settings and is counted.
    #[test]
    fn orphan_pattern_and_missing_pattern_are_counted() {
        let mut lost = brush("Lost Texture", None);
        lost.texture = Some(texture("missing"));
        let out = map_file(&file_with_patterns(
            vec![],
            vec![lost],
            vec![pattern("unused")],
        ));
        assert!(out.patterns.is_empty(), "nothing references the record");
        assert_eq!(out.orphan_patterns, 1);
        assert_eq!(out.textures_missing_pattern, 1);
        assert_eq!(out.entries.len(), 1);
        assert!(out.entries[0].texture.is_none());
        assert_eq!(out.entries[0].preset.name, "Lost Texture");
        assert!(out.entries[0].preset.validate().is_ok());
    }

    /// A brush without a texture never touches the pattern table.
    #[test]
    fn untextured_brush_has_no_texture_and_no_patterns() {
        let out = map_file(&file_with_patterns(
            vec![],
            vec![brush("Plain", None)],
            vec![pattern("unused")],
        ));
        assert!(out.entries[0].texture.is_none());
        assert!(out.patterns.is_empty());
        assert_eq!(out.orphan_patterns, 1);
        assert_eq!(out.textures_missing_pattern, 0);
    }
}
