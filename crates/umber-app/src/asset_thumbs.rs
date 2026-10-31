//! Content-keyed preview thumbnails for the Assets browser (the R-C1
//! follow-up "thumbnails"): each smart-material / smart-mask row gets a
//! small procedural swatch so the library reads at a glance.
//!
//! The swatches are HONEST previews, not renders: a mask swatch evaluates
//! the preset's real [`SmartMask::compose_scalar`] per pixel with a
//! procedural stand-in for each source (constants evaluate as-is, noise
//! generators get seeded value noise, mesh-map generators get a canonical
//! pattern per map kind, anchors read mid-gray); a material swatch
//! composites the template's fill layers bottom-to-top over neutral gray
//! using the same generator stand-ins (opacity + Normal blend only; other
//! blend modes fall back to Normal — the swatch is a hint, not a render).
//!
//! The panel caches textures keyed by (kind, name, content hash of the
//! preset's JSON) — a changed preset file regenerates its swatch.

use egui::ColorImage;
use umber_core::smart::{MaskSource, SmartMask, SmartMaterial};

/// Swatch edge in pixels (matches the layer thumbnails' 32 px chips).
pub const THUMB_PX: u32 = 32;

/// Deterministic lattice hash → [0, 1).
fn lattice(ix: i32, iy: i32, seed: u32) -> f32 {
    let mut h = seed
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add((ix as u32).wrapping_mul(0x85EB_CA6B))
        .wrapping_add((iy as u32).wrapping_mul(0xC2B2_AE35));
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    (h & 0x00FF_FFFF) as f32 / 16_777_216.0
}

/// Two-octave value noise in [0, 1).
fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (fx, fy) = (x.floor() as i32, y.floor() as i32);
    let (tx, ty) = (x - fx as f32, y - fy as f32);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let a = lattice(fx, fy, seed);
    let b = lattice(fx + 1, fy, seed);
    let c = lattice(fx, fy + 1, seed);
    let d = lattice(fx + 1, fy + 1, seed);
    let ab = a + (b - a) * sx;
    let cd = c + (d - c) * sx;
    ab + (cd - ab) * sy
}

fn seed_of(a: &str, b: &str) -> u32 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    a.hash(&mut h);
    b.hash(&mut h);
    h.finish() as u32
}

/// A canonical grayscale pattern per mesh-map kind, so different maps
/// read differently in a swatch. `u`/`v` in [0, 1].
fn map_pattern(map_name: &str, u: f32, v: f32, seed: u32) -> f32 {
    match map_name.to_ascii_lowercase().as_str() {
        // AO / cavity: bright centre, dark rim.
        "ao" | "cavity" => {
            let dx = u - 0.5;
            let dy = v - 0.5;
            (1.0 - (dx * dx + dy * dy).sqrt() * 1.6).clamp(0.0, 1.0)
        }
        // Curvature: wavy bands.
        "curvature" => 0.5 + 0.25 * (u * 14.0).sin() + 0.25 * (v * 9.0).sin(),
        // ID: hard color-ish bands (quantized here to gray steps).
        "id" => {
            let band = ((u * 5.0).floor() as i32 + (v * 3.0).floor() as i32).rem_euclid(3);
            [0.85, 0.45, 0.2][band as usize]
        }
        // Normals: a diagonal gradient.
        "normal" | "world normal" | "bent normal" => (0.25 + 0.5 * (u + v) / 2.0).clamp(0.0, 1.0),
        // Anything else: seeded noise.
        _ => value_noise(u * 6.0, v * 6.0, seed),
    }
}

/// The procedural stand-in for one mask source at normalized `(u, v)`.
fn source_value(source: &MaskSource, u: f32, v: f32) -> f32 {
    match source {
        MaskSource::Constant { value } => value.clamp(0.0, 1.0),
        MaskSource::Generator { node_def, map_name } => {
            let seed = seed_of(node_def, map_name);
            if node_def.to_ascii_lowercase().contains("noise") {
                value_noise(u * 6.0, v * 6.0, seed)
            } else {
                map_pattern(map_name, u, v, seed)
            }
        }
        // Anchors read other layers' planes — no mesh in a swatch, so
        // mid-gray (the compose math still runs, honestly labeled).
        MaskSource::Anchor { .. } => 0.5,
    }
}

/// Renders a smart mask's swatch. `None` when the preset has no ops
/// (the loader already refuses those at scan time; belt-and-braces).
pub fn mask_preview(mask: &SmartMask, size: u32) -> Option<ColorImage> {
    let size = size.max(1) as usize;
    let mut px = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            let g = mask
                .compose_scalar(&|s| source_value(s, u, v))
                .ok()?
                .clamp(0.0, 1.0);
            let g8 = (g * 255.0).round() as u8;
            let i = (y * size + x) * 4;
            px[i..i + 4].copy_from_slice(&[g8, g8, g8, 255]);
        }
    }
    Some(ColorImage::from_rgba_unmultiplied([size, size], &px))
}

/// Renders a smart material's swatch: the template's fill/paint layers
/// composited bottom-to-top over neutral gray (Normal blend; the
/// opacity applies; folder layers are skipped; generator-bound layers
/// sample the canonical map pattern as luminance).
pub fn material_preview(material: &SmartMaterial, size: u32) -> ColorImage {
    let size = size.max(1) as usize;
    let mut px = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            // Neutral gray base.
            let (mut r, mut g, mut b) = (0.5f32, 0.5, 0.5);
            for layer in &material.layers {
                use umber_core::smart::SmartLayerKind;
                if layer.kind == SmartLayerKind::Folder {
                    continue;
                }
                // Swatch color for this layer: the generator's map
                // pattern as luminance, or a plain light fill.
                let lum = match &layer.generator {
                    Some(binding) => {
                        if binding.node_def.to_ascii_lowercase().contains("noise") {
                            value_noise(
                                u * 6.0,
                                v * 6.0,
                                seed_of(&binding.node_def, &binding.map_name),
                            )
                        } else {
                            map_pattern(
                                &binding.map_name,
                                u,
                                v,
                                seed_of(&binding.node_def, &binding.map_name),
                            )
                        }
                    }
                    None => 0.75,
                };
                let a = layer.opacity.clamp(0.0, 1.0);
                // Normal blend only (the swatch is a hint, not a render).
                r = r + (lum - r) * a;
                g = g + (lum - g) * a;
                b = b + (lum - b) * a;
            }
            let i = (y * size + x) * 4;
            px[i] = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
            px[i + 1] = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
            px[i + 2] = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
            px[i + 3] = 255;
        }
    }
    ColorImage::from_rgba_unmultiplied([size, size], &px)
}

/// The panel-side cache key: kind byte + preset name + content hash of
/// the preset's own JSON (a changed file changes the swatch).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThumbKey {
    pub kind: u8,
    pub name: String,
    pub content: u64,
}

pub fn thumb_key(kind: u8, name: &str, preset_json: &str) -> ThumbKey {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    preset_json.hash(&mut h);
    ThumbKey {
        kind,
        name: name.to_owned(),
        content: h.finish(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_core::smart::{MaskOp, MaskOpKind, MaskOpParams, SmartLayerTemplate};

    fn const_mask(v: f32) -> SmartMask {
        SmartMask {
            name: "c".into(),
            version: 1,
            ops: vec![MaskOp {
                source: MaskSource::Constant { value: v },
                op: MaskOpKind::Multiply,
                params: MaskOpParams::default(),
            }],
        }
    }

    #[test]
    fn constant_mask_swatches_uniform() {
        let img = mask_preview(&const_mask(0.25), 8).expect("non-empty mask");
        assert_eq!(img.width(), 8);
        let px = img.as_raw();
        let first = px[0];
        assert!(px.chunks(4).all(|c| c[0] == first && c[3] == 255));
        assert!((first as f32 - 0.25 * 255.0).abs() <= 1.0);
    }

    #[test]
    fn noise_mask_swatches_vary() {
        let mask = SmartMask {
            name: "n".into(),
            version: 1,
            ops: vec![MaskOp {
                source: MaskSource::Generator {
                    node_def: "noise_perlin".into(),
                    map_name: "mask".into(),
                },
                op: MaskOpKind::Multiply,
                params: MaskOpParams::default(),
            }],
        };
        let img = mask_preview(&mask, 16).expect("non-empty mask");
        let px = img.as_raw();
        let distinct = px
            .chunks(4)
            .map(|c| c[0])
            .collect::<std::collections::HashSet<_>>();
        assert!(
            distinct.len() > 8,
            "a noise swatch must vary, got {distinct:?}"
        );
    }

    #[test]
    fn empty_mask_has_no_swatch() {
        let mask = SmartMask {
            name: "e".into(),
            version: 1,
            ops: vec![],
        };
        assert!(mask_preview(&mask, 8).is_none());
    }

    #[test]
    fn material_swatch_is_deterministic_and_sized() {
        let mat = SmartMaterial {
            name: "m".into(),
            version: 1,
            layers: vec![
                SmartLayerTemplate {
                    name: "base".into(),
                    kind: umber_core::smart::SmartLayerKind::Fill,
                    blend_mode: umber_core::layers::BlendMode::Normal,
                    opacity: 1.0,
                    mask: None,
                    generator: None,
                },
                SmartLayerTemplate {
                    name: "wear".into(),
                    kind: umber_core::smart::SmartLayerKind::Fill,
                    blend_mode: umber_core::layers::BlendMode::Normal,
                    opacity: 0.5,
                    mask: None,
                    generator: Some(umber_core::smart::GeneratorBinding {
                        node_def: "mesh_map_generator".into(),
                        map_name: "AO".into(),
                        channel: "baseColor".into(),
                    }),
                },
            ],
        };
        let a = material_preview(&mat, 16);
        let b = material_preview(&mat, 16);
        assert_eq!(a.as_raw(), b.as_raw(), "swatches are deterministic");
        assert_eq!(a.size, [16, 16]);
        // The AO pattern varies across the swatch.
        let distinct = a
            .as_raw()
            .chunks(4)
            .map(|c| c[0])
            .collect::<std::collections::HashSet<_>>();
        assert!(distinct.len() > 4);
    }

    #[test]
    fn thumb_key_tracks_content_not_just_name() {
        let k1 = thumb_key(1, "metal", "{\"a\":1}");
        let k2 = thumb_key(1, "metal", "{\"a\":2}");
        let k3 = thumb_key(2, "metal", "{\"a\":1}");
        assert_ne!(k1, k2);
        assert_ne!(k1, k3);
    }
}
