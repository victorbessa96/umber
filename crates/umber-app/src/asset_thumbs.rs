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
use std::path::Path;
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

/// Renders a brush preset's swatch (UI/UX pass U3, the Blender 4.3
/// asset-shelf pattern: every brush shows what it paints): one S-shaped
/// stroke of the preset's own dabs over a dark card. Each dab is the
/// preset's falloff at its hardness, in its color, at its alpha, with
/// the stroke tapering at both ends the way a pressure stroke does.
/// Pure CPU, deterministic, `size` x `size` px.
#[cfg(test)]
pub fn brush_preview(preset: &umber_brush::preset::BrushPreset, size: u32) -> ColorImage {
    brush_preview_on(preset, size, [DARK_CARD; 3])
}

/// The dark swatch card (sRGB, 0..1): the original card, kept by tests.
#[cfg(test)]
pub const DARK_CARD: f32 = 0.16;

/// [`brush_preview`] over a `card` colour (sRGB, 0..1): the panel draws
/// the swatch on the theme's raised surface (U4), so a Light shelf shows
/// light cards instead of black boxes. The eraser checker is the same in
/// every theme; its cut shows the card colour.
pub fn brush_preview_on(
    preset: &umber_brush::preset::BrushPreset,
    size: u32,
    card_rgb: [f32; 3],
) -> ColorImage {
    let size = size.max(1) as usize;
    let params = &preset.params;
    let [cr, cg, cb, ca] = params.color;
    // An eraser preset carries a zero colour: draw what it does (a cut
    // through a light checker, the "removed" convention), not black ink.
    let eraser = ca <= 1e-4;
    let to_srgb = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    };
    // Premultiplied stroke colour; un-premultiply for the over-blend.
    let ink = if eraser {
        [0.0; 3]
    } else {
        [to_srgb(cr / ca), to_srgb(cg / ca), to_srgb(cb / ca)]
    };
    let mut acc = vec![0f32; size * size];
    let n = size as f32;
    // Sparse spacing shows as separate dabs, dense as a smooth stroke:
    // the dab count follows the preset's dabs-per-radius.
    let dabs = (preset.spacing.dabs_per_radius * 4.0).clamp(10.0, 64.0) as usize;
    let max_r = n * 0.16;
    for k in 0..dabs {
        let t = k as f32 / (dabs - 1) as f32;
        let cx = n * (0.14 + 0.72 * t);
        let cy = n * (0.5 + 0.18 * (t * std::f32::consts::TAU).sin());
        // Pressure taper in, taper out.
        let pressure = (t * std::f32::consts::PI).sin().max(0.15);
        let r = (max_r * (0.45 + 0.55 * pressure)).max(1.0);
        // Per-dab alpha scaled so dense and sparse presets reach a
        // comparable stroke opacity.
        let dab_alpha = ((params.alpha * pressure).clamp(0.0, 1.0) * 12.0 / dabs as f32).min(1.0);
        let (x0, x1) = (
            ((cx - r).floor().max(0.0)) as usize,
            ((cx + r).ceil().min(n - 1.0)) as usize,
        );
        let (y0, y1) = (
            ((cy - r).floor().max(0.0)) as usize,
            ((cy + r).ceil().min(n - 1.0)) as usize,
        );
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt() / r;
                if d >= 1.0 {
                    continue;
                }
                let f = params
                    .falloff
                    .eval_with(d, params.hardness, &params.falloff_custom)
                    .clamp(0.0, 1.0);
                let a = &mut acc[y * size + x];
                // Build-up accumulation (over), capped at 1.
                *a = *a + (1.0 - *a) * f * dab_alpha;
            }
        }
    }
    let card = |x: usize, y: usize, c: usize| -> f32 {
        if eraser {
            // A light checker the stroke cuts through.
            let cell = (size / 8).max(1);
            if ((x / cell) + (y / cell)).is_multiple_of(2) {
                0.78
            } else {
                0.62
            }
        } else {
            card_rgb[c]
        }
    };
    let mut px = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let i = y * size + x;
            let a = acc[i].clamp(0.0, 1.0);
            for c in 0..3 {
                let base = card(x, y, c);
                let target = if eraser { card_rgb[c] } else { ink[c] };
                px[i * 4 + c] = ((base + (target - base) * a) * 255.0).round() as u8;
            }
            px[i * 4 + 3] = 255;
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

/// Box-averages an RGBA8 image down to `size` x `size` (the swatch edge):
/// each output pixel is the mean of its source block.
fn downsample_rgba8(px: &[u8], w: u32, h: u32, size: u32) -> ColorImage {
    let size = size.max(1) as usize;
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let mut out = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let x0 = x * w / size;
            let x1 = (((x + 1) * w / size).max(x0 + 1)).min(w);
            let y0 = y * h / size;
            let y1 = (((y + 1) * h / size).max(y0 + 1)).min(h);
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = (sy * w + sx) * 4;
                    for (c, a) in acc.iter_mut().enumerate() {
                        *a += px[i + c] as u32;
                    }
                    n += 1;
                }
            }
            let i = (y * size + x) * 4;
            for c in 0..4 {
                out[i + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    ColorImage::from_rgba_unmultiplied([size, size], &out)
}

/// A PNG file's downsampled swatch (K2) — the brush presets'
/// `<stem>.stamp.png` tips, shown in the Assets browser's brush rows when
/// the sidecar exists.
pub fn png_file_preview(path: &Path, size: u32) -> Option<ColorImage> {
    let (w, h, rgba) = umber_export::png::read_png_rgba8(path).ok()?;
    Some(downsample_rgba8(&rgba, w, h, size))
}

/// An environment map's downsampled equirect swatch (K1). LDR PNGs
/// average straight; HDR EXRs get a per-channel Reinhard tone map plus
/// gamma first — the swatch is a hint, not a render, the same honesty
/// rule as the mask/material swatches.
pub fn env_preview(path: &Path, size: u32) -> Option<ColorImage> {
    match crate::env::format_for_path(path)? {
        umber_gpu::EnvFormat::Rgba8Unorm => png_file_preview(path, size),
        umber_gpu::EnvFormat::Rgba32Float => {
            let (w, h, floats) = umber_export::exr::read_exr_rgba_f32(path).ok()?;
            let mut rgba = vec![0u8; floats.len()];
            for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
                for c in 0..3 {
                    let v = floats[i * 4 + c].max(0.0);
                    let t = v / (1.0 + v); // Reinhard
                    px[c] = (t.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8;
                }
                px[3] = 255;
            }
            Some(downsample_rgba8(&rgba, w, h, size))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// U4: the default swatch is the dark card exactly; a light card
    /// lightens the background and keeps the stroke.
    #[test]
    fn swatch_card_follows_the_given_colour() {
        let p = brush("hard-round");
        let dark = brush_preview(&p, 32);
        assert_eq!(dark.pixels, brush_preview_on(&p, 32, [DARK_CARD; 3]).pixels);
        let light = brush_preview_on(&p, 32, [0.9; 3]);
        // Corner = bare card.
        assert_eq!(dark.pixels[0].r(), (DARK_CARD * 255.0).round() as u8);
        assert_eq!(light.pixels[0].r(), (0.9f32 * 255.0).round() as u8);
        assert_ne!(dark.pixels, light.pixels);
    }

    fn brush(name: &str) -> umber_brush::preset::BrushPreset {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/brushes")
            .join(format!("{name}.umberbrush"));
        umber_brush::preset::BrushPreset::from_json(&std::fs::read_to_string(path).unwrap())
            .unwrap()
    }

    #[test]
    fn eraser_previews_cut_through_a_light_card() {
        let img = brush_preview(&brush("eraser-hard"), 48);
        let mean: f32 =
            img.pixels.iter().map(|p| p.r() as f32).sum::<f32>() / img.pixels.len() as f32;
        assert!(
            mean > 100.0,
            "an eraser swatch reads light, not as black ink: {mean}"
        );
    }

    fn coverage(img: &ColorImage) -> usize {
        // Pixels lifted off the card colour (0.16 * 255 = 41).
        img.pixels
            .iter()
            .filter(|p| p.r().abs_diff(41) > 8 || p.g().abs_diff(41) > 8 || p.b().abs_diff(41) > 8)
            .count()
    }

    #[test]
    fn brush_previews_are_deterministic_and_show_the_stroke() {
        let p = brush("hard-round");
        let a = brush_preview(&p, 64);
        assert_eq!(a.size, [64, 64]);
        assert_eq!(a.pixels, brush_preview(&p, 64).pixels, "deterministic");
        let painted = coverage(&a);
        assert!(painted > 64 * 64 / 20, "a visible stroke: {painted} px");
        assert!(
            painted < 64 * 64 * 3 / 4,
            "the card still shows: {painted} px"
        );
    }

    #[test]
    fn brush_previews_tell_presets_apart() {
        // Every shipped preset renders, and no two shipped presets share
        // a swatch (the shelf's whole point).
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/brushes");
        let mut seen: Vec<(String, Vec<egui::Color32>)> = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("umberbrush") {
                continue;
            }
            let p = umber_brush::preset::BrushPreset::from_json(
                &std::fs::read_to_string(&path).unwrap(),
            )
            .unwrap();
            let img = brush_preview(&p, 48);
            for (other, px) in &seen {
                assert_ne!(
                    *px, img.pixels,
                    "{} and {other} draw the same swatch",
                    p.name
                );
            }
            seen.push((p.name.clone(), img.pixels));
        }
        assert!(seen.len() >= 10, "{} presets", seen.len());
    }
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
            parameters: Vec::new(),
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
                        param: None,
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

    #[test]
    fn downsample_box_averages_blocks() {
        // 4x4 -> 2x2: each output pixel is the mean of its 2x2 block.
        let mut px = vec![0u8; 4 * 4 * 4];
        for y in 0..4usize {
            for x in 0..4usize {
                let i = (y * 4 + x) * 4;
                let v = ((y * 4 + x) * 16) as u8;
                px[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let img = downsample_rgba8(&px, 4, 4, 2);
        assert_eq!(img.size, [2, 2]);
        // Top-left block: pixels 0, 1, 4, 5 -> (0+16+64+80)/4 = 40.
        assert_eq!(img.pixels[0].r(), 40);
        // Bottom-right block: 10, 11, 14, 15 -> (160+176+224+240)/4 = 200.
        assert_eq!(img.pixels[3].r(), 200);
    }

    #[test]
    fn png_file_preview_reads_and_downsamples_a_real_file() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/env/studio-neutral.png");
        let img = png_file_preview(&p, 16).expect("the committed studio map decodes");
        assert_eq!(img.size, [16, 16]);
        assert_eq!(
            img.pixels,
            png_file_preview(&p, 16).unwrap().pixels,
            "deterministic"
        );
        assert!(png_file_preview(Path::new("/nonexistent/x.png"), 16).is_none());
    }

    #[test]
    fn env_preview_gates_on_format_and_handles_ldr_pngs() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/env/studio-neutral.png");
        let img = env_preview(&p, 8).expect("LDR png env swatch");
        assert_eq!(img.size, [8, 8]);
        let txt = std::env::temp_dir().join(format!("umber-env-prev-{}.txt", std::process::id()));
        std::fs::write(&txt, b"nope").unwrap();
        assert!(env_preview(&txt, 8).is_none(), "non-env extension is None");
        let _ = std::fs::remove_file(&txt);
    }
}
