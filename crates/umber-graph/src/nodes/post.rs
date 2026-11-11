//! Post-process nodes: `glare`, `lens_flare`, `chromatic_aberration`,
//! `film_grain` (census gap R-E5, `docs/research/spp-parity-census-2026-10-16.md`
//! §16; brief `docs/loop/briefs/2026-11-10-post-nodes.md`).
//!
//! Every node consumes exactly one Image input and produces an `Image` the
//! same size as its input (they transform, they don't generate). RGB is
//! processed in straight (non-premultiplied) 0..1 float and rounded
//! half-up back to bytes; alpha passes through untouched. Every eval is a
//! pure function of (input bytes, params): `film_grain`'s noise comes from
//! an integer hash of `(seed, cell x, cell y, channel)`, so a replay with
//! the same seed is byte-exact on any host.

use crate::value::ImageBuffer;
use crate::{EvalContext, EvalError, NodeImpl, NodeOutput, ParamValue};
use std::sync::Arc;

fn find_param<'a>(params: &'a [(String, ParamValue)], name: &str) -> Option<&'a ParamValue> {
    params.iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

fn bad(param: &str) -> EvalError {
    EvalError::BadParam {
        node: 0,
        param: param.into(),
    }
}

fn single_image(inputs: &[(String, NodeOutput)]) -> Result<&ImageBuffer, EvalError> {
    match inputs.len() {
        1 => match &inputs[0].1 {
            NodeOutput::Image(buf) => Ok(buf),
            other => Err(EvalError::TypeMismatch {
                node: 0,
                expected: "Image".into(),
                got: other.kind().into(),
            }),
        },
        0 => Err(EvalError::MissingInput {
            node: 0,
            input: "in".into(),
        }),
        n => Err(bad(&format!("expected exactly 1 input, got {n}"))),
    }
}

fn float_param(
    params: &[(String, ParamValue)],
    name: &str,
    default: f32,
) -> Result<f32, EvalError> {
    let v = match find_param(params, name) {
        None => default,
        Some(ParamValue::Float(v)) => *v,
        Some(_) => return Err(bad(name)),
    };
    if v.is_finite() {
        Ok(v)
    } else {
        Err(bad(name))
    }
}

fn int_param(params: &[(String, ParamValue)], name: &str, default: i32) -> Result<i32, EvalError> {
    match find_param(params, name) {
        None => Ok(default),
        Some(ParamValue::Int(v)) => Ok(*v),
        Some(_) => Err(bad(name)),
    }
}

fn bool_param(
    params: &[(String, ParamValue)],
    name: &str,
    default: bool,
) -> Result<bool, EvalError> {
    match find_param(params, name) {
        None => Ok(default),
        Some(ParamValue::Bool(v)) => Ok(*v),
        Some(_) => Err(bad(name)),
    }
}

fn vec2_param(
    params: &[(String, ParamValue)],
    name: &str,
    default: [f32; 2],
) -> Result<[f32; 2], EvalError> {
    let v = match find_param(params, name) {
        None => default,
        Some(ParamValue::Vec2(v)) => *v,
        Some(_) => return Err(bad(name)),
    };
    if v.iter().all(|c| c.is_finite()) {
        Ok(v)
    } else {
        Err(bad(name))
    }
}

fn color_param(
    params: &[(String, ParamValue)],
    name: &str,
    default: [f32; 3],
) -> Result<[f32; 3], EvalError> {
    let v = match find_param(params, name) {
        None => default,
        Some(ParamValue::Color(v) | ParamValue::Vec3(v)) => *v,
        Some(_) => return Err(bad(name)),
    };
    if v.iter().all(|c| c.is_finite()) {
        Ok(v)
    } else {
        Err(bad(name))
    }
}

/// Round-half-up float→byte (the merged convention, see `filter.rs`).
fn byte_round(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Rec.709 luminance of a 0..1 RGB triple.
fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// The soft-knee bright-pass weight (the Unity/Unreal bloom curve):
/// with `knee == 0` it is the hard step `max(l - t, 0) / l`; a positive
/// knee eases the cut over `[t - knee, t + knee]` with a quadratic.
/// The result multiplies the pixel's RGB (hue is kept).
fn bright_weight(l: f32, threshold: f32, knee: f32) -> f32 {
    if l <= 0.0 {
        return 0.0;
    }
    let hard = (l - threshold).max(0.0);
    let soft = if knee > 0.0 {
        let s = (l - threshold + knee).clamp(0.0, 2.0 * knee);
        s * s / (4.0 * knee)
    } else {
        0.0
    };
    hard.max(soft) / l
}

/// The input's RGB as three 0..1 float planes plus the bright-pass
/// planes (`rgb * bright_weight(luma)`).
fn bright_planes(src: &ImageBuffer, threshold: f32, knee: f32) -> [Vec<f32>; 3] {
    let n = src.width as usize * src.height as usize;
    let mut out = [vec![0.0f32; n], vec![0.0f32; n], vec![0.0f32; n]];
    for (i, px) in src.data.chunks_exact(4).enumerate() {
        let rgb = [
            f32::from(px[0]) / 255.0,
            f32::from(px[1]) / 255.0,
            f32::from(px[2]) / 255.0,
        ];
        let w = bright_weight(luma(rgb), threshold, knee);
        for c in 0..3 {
            out[c][i] = rgb[c] * w;
        }
    }
    out
}

/// One edge-clamped 1-D box average over `line` (O(n) running sum, so a
/// hostile radius cannot hang the engine).
fn box_line(line: &[f32], radius: usize) -> Vec<f32> {
    let n = line.len();
    if radius == 0 || n == 0 {
        return line.to_vec();
    }
    let at = |i: i64| line[i.clamp(0, n as i64 - 1) as usize];
    let r = radius as i64;
    let k = (2 * radius + 1) as f32;
    let mut sum: f64 = (-r..=r).map(|d| f64::from(at(d))).sum();
    let mut out = vec![0.0f32; n];
    out[0] = (sum / f64::from(k)) as f32;
    for (x, slot) in out.iter_mut().enumerate().skip(1) {
        let xi = x as i64;
        sum += f64::from(at(xi + r)) - f64::from(at(xi - r - 1));
        *slot = (sum / f64::from(k)) as f32;
    }
    out
}

/// Separable box blur of one `w`×`h` float plane.
fn box_blur_plane(plane: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    if radius == 0 {
        return plane.to_vec();
    }
    let mut tmp = vec![0.0f32; w * h];
    for y in 0..h {
        let row = box_line(&plane[y * w..(y + 1) * w], radius);
        tmp[y * w..(y + 1) * w].copy_from_slice(&row);
    }
    let mut out = vec![0.0f32; w * h];
    let mut col = vec![0.0f32; h];
    for x in 0..w {
        for (y, slot) in col.iter_mut().enumerate() {
            *slot = tmp[y * w + x];
        }
        for (y, v) in box_line(&col, radius).into_iter().enumerate() {
            out[y * w + x] = v;
        }
    }
    out
}

/// `src + add[c] * gain[c]` per RGB channel, alpha kept.
fn add_planes(
    src: &ImageBuffer,
    add: &[Vec<f32>; 3],
    gain: [f32; 3],
) -> Result<ImageBuffer, EvalError> {
    let mut data = Vec::with_capacity(src.data.len());
    for (i, px) in src.data.chunks_exact(4).enumerate() {
        for c in 0..3 {
            data.push(byte_round(f32::from(px[c]) / 255.0 + gain[c] * add[c][i]));
        }
        data.push(px[3]);
    }
    Ok(ImageBuffer::new(src.width, src.height, data)?)
}

/// `glare`: soft-knee highlight glow. The bright pass
/// ([`bright_weight`] on Rec.709 luma) is box-blurred and added back:
/// `out = in + intensity * blur(bright(in))`.
///
/// Params: `threshold: Float` (default 0.8, clamped to `[0, 1]`),
/// `knee: Float` (default 0.1, clamped `>= 0`), `intensity: Float`
/// (default 1.0, clamped `>= 0`), `radius: Int` (default 4 px, clamped to
/// `[0, 256]`). Output: `Image`, input size.
pub struct GlareNode;

impl NodeImpl for GlareNode {
    fn eval(
        &self,
        inputs: Vec<(String, NodeOutput)>,
        params: &[(String, ParamValue)],
        _ctx: &EvalContext,
    ) -> Result<NodeOutput, EvalError> {
        let src = single_image(&inputs)?;
        let threshold = float_param(params, "threshold", 0.8)?.clamp(0.0, 1.0);
        let knee = float_param(params, "knee", 0.1)?.max(0.0);
        let intensity = float_param(params, "intensity", 1.0)?.max(0.0);
        let radius = int_param(params, "radius", 4)?.clamp(0, 256) as usize;
        let (w, h) = (src.width as usize, src.height as usize);
        let bright = bright_planes(src, threshold, knee);
        let glow = bright.map(|p| box_blur_plane(&p, w, h, radius));
        Ok(NodeOutput::Image(add_planes(src, &glow, [intensity; 3])?))
    }
}

/// Symmetric horizontal exponential streak of one row:
/// `s[x] = Σ_d b[x + d] * decay^|d|` (pixels past the edge are zero),
/// computed in O(n) as a causal + anti-causal recursion minus the
/// double-counted centre tap.
fn streak_line(line: &[f32], decay: f32) -> Vec<f32> {
    let n = line.len();
    let mut fwd = vec![0.0f32; n];
    let mut acc = 0.0f32;
    for (x, v) in line.iter().enumerate() {
        acc = v + decay * acc;
        fwd[x] = acc;
    }
    acc = 0.0;
    for x in (0..n).rev() {
        acc = line[x] + decay * acc;
        fwd[x] += acc - line[x];
    }
    fwd
}

/// `lens_flare`: an anamorphic (horizontal) streak from the highlights.
/// The bright pass (hard-ish threshold with a small fixed knee of 0.05)
/// is smeared along each row with an exponential falloff
/// ([`streak_line`]), normalized by the falloff's total weight
/// `(1 + decay) / (1 - decay)`, tinted, and added back.
///
/// Params: `threshold: Float` (default 0.9, clamped to `[0, 1]`),
/// `intensity: Float` (default 1.0, clamped `>= 0`), `decay: Float`
/// (default 0.95 — the per-pixel falloff, clamped to `[0, 0.999]`; 0
/// means no streak, only the bright pass), `tint: Color` (default
/// `[0.6, 0.8, 1.0]`, the classic blue anamorphic streak). Output:
/// `Image`, input size.
pub struct LensFlareNode;

impl NodeImpl for LensFlareNode {
    fn eval(
        &self,
        inputs: Vec<(String, NodeOutput)>,
        params: &[(String, ParamValue)],
        _ctx: &EvalContext,
    ) -> Result<NodeOutput, EvalError> {
        let src = single_image(&inputs)?;
        let threshold = float_param(params, "threshold", 0.9)?.clamp(0.0, 1.0);
        let intensity = float_param(params, "intensity", 1.0)?.max(0.0);
        let decay = float_param(params, "decay", 0.95)?.clamp(0.0, 0.999);
        let tint = color_param(params, "tint", [0.6, 0.8, 1.0])?;
        let w = src.width as usize;
        // Normalize so a uniform bright row streaks to its own value
        // (the gain is then an honest "intensity").
        let norm = (1.0 - decay) / (1.0 + decay);
        let bright = bright_planes(src, threshold, 0.05);
        let streak = bright.map(|p| {
            let mut out = Vec::with_capacity(p.len());
            for row in p.chunks_exact(w) {
                out.extend(streak_line(row, decay).into_iter().map(|v| v * norm));
            }
            out
        });
        let gain = tint.map(|t| t.max(0.0) * intensity);
        Ok(NodeOutput::Image(add_planes(src, &streak, gain)?))
    }
}

/// Bilinear sample of channel `c` at pixel-space `(x, y)` (pixel centres
/// at integer + 0.5), edge-clamped.
fn sample_bilinear(src: &ImageBuffer, c: usize, x: f32, y: f32) -> f32 {
    let (w, h) = (src.width as i64, src.height as i64);
    let fx = x - 0.5;
    let fy = y - 0.5;
    let x0 = fx.floor();
    let y0 = fy.floor();
    let tx = fx - x0;
    let ty = fy - y0;
    let at = |xi: i64, yi: i64| {
        let xi = xi.clamp(0, w - 1) as usize;
        let yi = yi.clamp(0, h - 1) as usize;
        f32::from(src.data[(yi * w as usize + xi) * 4 + c])
    };
    let (x0, y0) = (x0 as i64, y0 as i64);
    let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1, y0) * tx;
    let bot = at(x0, y0 + 1) * (1.0 - tx) + at(x0 + 1, y0 + 1) * tx;
    top * (1.0 - ty) + bot * ty
}

/// `chromatic_aberration`: lateral (radial) chromatic aberration. For
/// each pixel at offset `d` from `center`, red samples the input at
/// `center + d * (1 + strength)` and blue at `center + d * (1 - strength)`
/// (bilinear, edge-clamped); green and alpha pass through. The shift
/// therefore grows linearly with distance from the centre and is zero at
/// it.
///
/// Params: `strength: Float` (default 0.005, clamped to `[-0.5, 0.5]`;
/// negative swaps the fringe colours), `center: Vec2` (default
/// `[0.5, 0.5]`, UV of the optical centre). `strength == 0` returns the
/// input byte-identical. Output: `Image`, input size.
pub struct ChromaticAberrationNode;

impl NodeImpl for ChromaticAberrationNode {
    fn eval(
        &self,
        inputs: Vec<(String, NodeOutput)>,
        params: &[(String, ParamValue)],
        _ctx: &EvalContext,
    ) -> Result<NodeOutput, EvalError> {
        let src = single_image(&inputs)?;
        let strength = float_param(params, "strength", 0.005)?.clamp(-0.5, 0.5);
        let center = vec2_param(params, "center", [0.5, 0.5])?;
        if strength == 0.0 {
            return Ok(NodeOutput::Image(src.clone()));
        }
        let (w, h) = (src.width, src.height);
        let cx = center[0] * w as f32;
        let cy = center[1] * h as f32;
        let mut data = Vec::with_capacity(src.data.len());
        for y in 0..h {
            for x in 0..w {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let (dx, dy) = (px - cx, py - cy);
                let r = sample_bilinear(
                    src,
                    0,
                    cx + dx * (1.0 + strength),
                    cy + dy * (1.0 + strength),
                );
                let b = sample_bilinear(
                    src,
                    2,
                    cx + dx * (1.0 - strength),
                    cy + dy * (1.0 - strength),
                );
                let i = ((y * w + x) * 4) as usize;
                data.push(r.clamp(0.0, 255.0).round() as u8);
                data.push(src.data[i + 1]);
                data.push(b.clamp(0.0, 255.0).round() as u8);
                data.push(src.data[i + 3]);
            }
        }
        Ok(NodeOutput::Image(ImageBuffer::new(w, h, data)?))
    }
}

/// The integer hash behind `film_grain` (the `lowbias32` finalizer over a
/// combined key). Pure `u32` wrapping math: identical on every host.
fn grain_hash(seed: i32, x: u32, y: u32, c: u32) -> u32 {
    let mut h = (seed as u32)
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(x.wrapping_mul(0x85EB_CA6B))
        .wrapping_add(y.wrapping_mul(0xC2B2_AE35))
        .wrapping_add(c.wrapping_mul(0x27D4_EB2F));
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    h
}

/// Zero-mean triangular noise in `[-1, 1]` (sum of two uniforms — a
/// softer, more film-like distribution than a flat uniform).
fn grain_value(seed: i32, x: u32, y: u32, c: u32) -> f32 {
    let a = grain_hash(seed, x, y, c);
    let b = grain_hash(seed ^ 0x5bd1_e995_u32 as i32, x, y, c);
    let ua = (a >> 8) as f32 / 16_777_216.0;
    let ub = (b >> 8) as f32 / 16_777_216.0;
    ua + ub - 1.0
}

/// `film_grain`: seeded, deterministic additive grain:
/// `out = in + amount * n`, `n` triangular in `[-1, 1]` from
/// [`grain_hash`] of `(seed, x / size, y / size, channel)`.
///
/// Params: `amount: Float` (default 0.05, clamped to `[0, 1]`), `seed:
/// Int` (default 0 — the replay key: same seed + same input ⇒ same
/// bytes), `size: Int` (default 1 px grain cell, clamped to `[1, 64]`),
/// `monochrome: Bool` (default true: one noise value shared by R, G, B;
/// false draws per channel). `amount == 0` returns the input
/// byte-identical. Alpha passes through. Output: `Image`, input size.
pub struct FilmGrainNode;

impl NodeImpl for FilmGrainNode {
    fn eval(
        &self,
        inputs: Vec<(String, NodeOutput)>,
        params: &[(String, ParamValue)],
        _ctx: &EvalContext,
    ) -> Result<NodeOutput, EvalError> {
        let src = single_image(&inputs)?;
        let amount = float_param(params, "amount", 0.05)?.clamp(0.0, 1.0);
        let seed = int_param(params, "seed", 0)?;
        let size = int_param(params, "size", 1)?.clamp(1, 64) as u32;
        let mono = bool_param(params, "monochrome", true)?;
        if amount == 0.0 {
            return Ok(NodeOutput::Image(src.clone()));
        }
        let (w, h) = (src.width, src.height);
        let mut data = Vec::with_capacity(src.data.len());
        for y in 0..h {
            for x in 0..w {
                let (gx, gy) = (x / size, y / size);
                let i = ((y * w + x) * 4) as usize;
                for c in 0..3u32 {
                    let n = grain_value(seed, gx, gy, if mono { 0 } else { c });
                    let v = f32::from(src.data[i + c as usize]) / 255.0;
                    data.push(byte_round(v + amount * n));
                }
                data.push(src.data[i + 3]);
            }
        }
        Ok(NodeOutput::Image(ImageBuffer::new(w, h, data)?))
    }
}

/// Registers `glare`, `lens_flare`, `chromatic_aberration`, `film_grain`.
pub fn register_post_nodes(registry: &mut crate::NodeRegistry) {
    registry.register("glare", Arc::new(GlareNode));
    registry.register("lens_flare", Arc::new(LensFlareNode));
    registry.register("chromatic_aberration", Arc::new(ChromaticAberrationNode));
    registry.register("film_grain", Arc::new(FilmGrainNode));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> EvalContext {
        EvalContext::new(1, 1)
    }

    fn run(node: &dyn NodeImpl, img: &ImageBuffer, params: &[(String, ParamValue)]) -> ImageBuffer {
        match node
            .eval(
                vec![("in".into(), NodeOutput::Image(img.clone()))],
                params,
                &ctx(),
            )
            .expect("eval")
        {
            NodeOutput::Image(b) => b,
            other => panic!("expected Image, got {other:?}"),
        }
    }

    fn p(name: &str, v: ParamValue) -> (String, ParamValue) {
        (name.into(), v)
    }

    /// A deterministic non-constant fixture with varied alpha.
    fn varied(w: u32, h: u32) -> ImageBuffer {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[
                    ((x * 37 + y * 11) % 256) as u8,
                    ((x * 13 + y * 59 + 7) % 256) as u8,
                    ((x * 71 + y * 3 + 21) % 256) as u8,
                    ((x * 5 + y * 17 + 128) % 256) as u8,
                ]);
            }
        }
        ImageBuffer::new(w, h, data).unwrap()
    }

    /// Black canvas with one white pixel at `(x, y)`.
    fn spot(w: u32, h: u32, x: u32, y: u32) -> ImageBuffer {
        let mut img = ImageBuffer::filled(w, h, [0, 0, 0, 255]).unwrap();
        img.set_pixel(x, y, [255, 255, 255, 255]);
        img
    }

    fn alpha(img: &ImageBuffer) -> Vec<u8> {
        img.data.chunks_exact(4).map(|p| p[3]).collect()
    }

    #[test]
    fn registry_lists_all_four() {
        let mut r = crate::NodeRegistry::seeded();
        register_post_nodes(&mut r);
        for name in ["glare", "lens_flare", "chromatic_aberration", "film_grain"] {
            assert!(r.get(name).is_some(), "{name} registered");
        }
    }

    #[test]
    fn bright_weight_soft_knee_curve() {
        // Hard cut with no knee.
        assert_eq!(bright_weight(0.5, 0.8, 0.0), 0.0);
        assert!((bright_weight(1.0, 0.8, 0.0) - 0.2).abs() < 1e-6);
        // In the knee: (l - t + k)^2 / 4k / l.
        let w = bright_weight(0.8, 0.8, 0.1);
        assert!((w - (0.1 * 0.1 / 0.4) / 0.8).abs() < 1e-6, "{w}");
        // Below the knee: zero; black: zero.
        assert_eq!(bright_weight(0.6, 0.8, 0.1), 0.0);
        assert_eq!(bright_weight(0.0, 0.0, 0.1), 0.0);
    }

    #[test]
    fn glare_golden_radius_zero_doubles_highlights() {
        // threshold 0, knee 0, radius 0: bright == input, out = 2*in clamped.
        let img = ImageBuffer::new(2, 1, vec![100, 50, 10, 200, 200, 200, 200, 9]).unwrap();
        let out = run(
            &GlareNode,
            &img,
            &[
                p("threshold", ParamValue::Float(0.0)),
                p("knee", ParamValue::Float(0.0)),
                p("radius", ParamValue::Int(0)),
            ],
        );
        assert_eq!(out.data, vec![200, 100, 20, 200, 255, 255, 255, 9]);
    }

    #[test]
    fn glare_spreads_a_highlight_and_spares_darks() {
        let img = spot(9, 9, 4, 4);
        let out = run(&GlareNode, &img, &[p("radius", ParamValue::Int(2))]);
        // Inside the 5x5 box the black pixels light up (equally), outside
        // they stay black; the bright pixel stays saturated.
        let glow = out.pixel(2, 2).unwrap();
        assert!(
            glow[0] > 0 && glow[0] == glow[1] && glow[1] == glow[2],
            "{glow:?}"
        );
        assert_eq!(out.pixel(6, 6).unwrap(), glow, "box glow is symmetric");
        assert_eq!(out.pixel(1, 4).unwrap(), [0, 0, 0, 255]);
        assert_eq!(out.pixel(4, 4).unwrap(), [255, 255, 255, 255]);
        // A dim image has no highlights: identity.
        let dim = ImageBuffer::filled(4, 4, [80, 80, 80, 77]).unwrap();
        assert_eq!(run(&GlareNode, &dim, &[]), dim);
    }

    #[test]
    fn glare_intensity_zero_is_identity_and_alpha_kept() {
        let img = varied(7, 5);
        let out = run(&GlareNode, &img, &[p("intensity", ParamValue::Float(0.0))]);
        assert_eq!(out, img);
        let out = run(&GlareNode, &img, &[p("threshold", ParamValue::Float(0.2))]);
        assert_eq!(alpha(&out), alpha(&img));
        // Glow only adds light.
        for (o, i) in out.data.iter().zip(&img.data) {
            assert!(o >= i);
        }
    }

    #[test]
    fn streak_line_matches_direct_sum() {
        let line = [0.0, 1.0, 0.0, 0.0, 0.5, 0.0];
        let decay = 0.5f32;
        let got = streak_line(&line, decay);
        for (x, g) in got.iter().enumerate() {
            let want: f32 = line
                .iter()
                .enumerate()
                .map(|(j, v)| v * decay.powi((x as i32 - j as i32).abs()))
                .sum();
            assert!((g - want).abs() < 1e-6, "x={x}: {g} vs {want}");
        }
    }

    #[test]
    fn lens_flare_streak_is_horizontal_and_tinted() {
        let img = spot(15, 5, 7, 2);
        let out = run(
            &LensFlareNode,
            &img,
            &[p("intensity", ParamValue::Float(8.0))],
        );
        // Same row, away from the source: lit, blue-dominant (default tint).
        let s = out.pixel(3, 2).unwrap();
        assert!(s[2] > 0 && s[2] >= s[1] && s[1] >= s[0], "{s:?}");
        // Mirrored about the source.
        assert_eq!(out.pixel(11, 2).unwrap(), s);
        // Falls off with distance.
        assert!(out.pixel(1, 2).unwrap()[2] <= s[2]);
        // Other rows untouched.
        for y in [0, 1, 3, 4] {
            for x in 0..15 {
                assert_eq!(out.pixel(x, y).unwrap(), [0, 0, 0, 255], "({x},{y})");
            }
        }
    }

    #[test]
    fn lens_flare_golden_decay_zero() {
        // decay 0: the streak is the bright pass itself (norm 1). A 0.6
        // grey with threshold 0.5 (knee 0.05): weight = 0.1 / 0.6, so the
        // bright pass is 0.1 per channel; the default tint adds
        // 0.06 / 0.08 / 0.10 → 0.66 / 0.68 / 0.70 → 168 / 173 / 179.
        let grey = ImageBuffer::new(1, 1, vec![153, 153, 153, 255]).unwrap();
        let params = [
            p("threshold", ParamValue::Float(0.5)),
            p("decay", ParamValue::Float(0.0)),
        ];
        assert_eq!(
            run(&LensFlareNode, &grey, &params).data,
            vec![168, 173, 179, 255]
        );
        // luma(0.8, 0, 0) = 0.17 sits below the knee: untouched.
        let red = ImageBuffer::new(1, 1, vec![204, 0, 0, 255]).unwrap();
        assert_eq!(run(&LensFlareNode, &red, &params), red);
    }

    #[test]
    fn chromatic_aberration_zero_strength_and_green_alpha_invariant() {
        let img = varied(9, 7);
        assert_eq!(
            run(
                &ChromaticAberrationNode,
                &img,
                &[p("strength", ParamValue::Float(0.0))]
            ),
            img
        );
        let out = run(
            &ChromaticAberrationNode,
            &img,
            &[p("strength", ParamValue::Float(0.1))],
        );
        for (o, i) in out.data.chunks_exact(4).zip(img.data.chunks_exact(4)) {
            assert_eq!(o[1], i[1], "green passes through");
            assert_eq!(o[3], i[3], "alpha passes through");
        }
        assert_ne!(out, img);
        // Odd size: the centre pixel sits on the optical centre and keeps
        // all its channels.
        assert_eq!(out.pixel(4, 3).unwrap(), img.pixel(4, 3).unwrap());
    }

    #[test]
    fn chromatic_aberration_golden_radial_shift() {
        // 5x1 ramp in R and B. strength 0.5, centre x = 2.5 (pixel 2).
        // Pixel 4 (centre 4.5, d = 2): R samples x = 2.5 + 3 = 5.5 → clamps
        // to pixel 4; B samples 2.5 + 1 = 3.5 → pixel 3. Pixel 3 (d = 1):
        // R at 4.0 → halfway 3..4; B at 3.0 → halfway 2..3. Pixel 0
        // (d = -2): R at -0.5 → clamps to pixel 0; B at 1.5 → pixel 1.
        let vals = [0u8, 40, 80, 120, 160];
        let mut data = Vec::new();
        for v in vals {
            data.extend_from_slice(&[v, 7, v, 255]);
        }
        let img = ImageBuffer::new(5, 1, data).unwrap();
        let out = run(
            &ChromaticAberrationNode,
            &img,
            &[p("strength", ParamValue::Float(0.5))],
        );
        let r: Vec<u8> = out.data.chunks_exact(4).map(|p| p[0]).collect();
        let b: Vec<u8> = out.data.chunks_exact(4).map(|p| p[2]).collect();
        assert_eq!(r, vec![0, 20, 80, 140, 160]);
        assert_eq!(b, vec![40, 60, 80, 100, 120]);
    }

    #[test]
    fn film_grain_is_seed_deterministic() {
        let img = ImageBuffer::filled(16, 16, [128, 128, 128, 200]).unwrap();
        let seeded = |s: i32| run(&FilmGrainNode, &img, &[p("seed", ParamValue::Int(s))]);
        assert_eq!(seeded(7), seeded(7), "same seed replays byte-exact");
        assert_ne!(seeded(7), seeded(8), "a different seed differs");
        assert_eq!(alpha(&seeded(7)), alpha(&img));
        // amount 0: identity.
        assert_eq!(
            run(&FilmGrainNode, &img, &[p("amount", ParamValue::Float(0.0))]),
            img
        );
    }

    #[test]
    fn film_grain_golden_hash_pins() {
        // Pins the hash so a refactor that changes replays fails loudly.
        assert_eq!(grain_hash(0, 0, 0, 0), 0);
        assert_eq!(grain_hash(1, 0, 0, 0), 0x01fc_e552);
        assert_eq!(grain_hash(7, 3, 5, 2), 0xc520_cfd0);
        assert_eq!(grain_hash(-1, 100, 200, 1), 0x86e5_4115);
        // Value golden: monochrome grain on mid-grey, amount 0.2, seed 3.
        let img = ImageBuffer::filled(4, 1, [128, 128, 128, 255]).unwrap();
        let out = run(
            &FilmGrainNode,
            &img,
            &[
                p("amount", ParamValue::Float(0.2)),
                p("seed", ParamValue::Int(3)),
            ],
        );
        for (x, px) in out.data.chunks_exact(4).enumerate() {
            let n = grain_value(3, x as u32, 0, 0);
            let want = byte_round(128.0 / 255.0 + 0.2 * n);
            assert_eq!(px[..3], [want; 3], "x={x}");
        }
        let reds: Vec<u8> = out.data.chunks_exact(4).map(|p| p[0]).collect();
        assert_eq!(reds, vec![107, 97, 124, 134]);
    }

    #[test]
    fn film_grain_statistics_and_modes() {
        let img = ImageBuffer::filled(64, 64, [128, 128, 128, 255]).unwrap();
        let out = run(&FilmGrainNode, &img, &[p("amount", ParamValue::Float(0.2))]);
        // Zero-mean: the average stays within 1.5 code values of 128
        // (the standard error of 4096 samples is ~0.33).
        let mean = out
            .data
            .chunks_exact(4)
            .map(|p| f64::from(p[0]))
            .sum::<f64>()
            / (64.0 * 64.0);
        assert!((mean - 128.0).abs() < 1.5, "mean {mean}");
        // Bounded: |delta| <= amount * 255 (+ rounding).
        for px in out.data.chunks_exact(4) {
            assert!((i32::from(px[0]) - 128).abs() <= 52, "{px:?}");
            assert!(px[0] == px[1] && px[1] == px[2], "monochrome");
        }
        // Colour grain decorrelates the channels.
        let col = run(
            &FilmGrainNode,
            &img,
            &[
                p("amount", ParamValue::Float(0.2)),
                p("monochrome", ParamValue::Bool(false)),
            ],
        );
        assert!(col
            .data
            .chunks_exact(4)
            .any(|p| p[0] != p[1] || p[1] != p[2]));
        // Grain size 4: every 4x4 cell is flat.
        let big = run(
            &FilmGrainNode,
            &img,
            &[
                p("amount", ParamValue::Float(0.2)),
                p("size", ParamValue::Int(4)),
            ],
        );
        for y in 0..64 {
            for x in 0..64 {
                assert_eq!(big.pixel(x, y), big.pixel(x / 4 * 4, y / 4 * 4));
            }
        }
    }

    #[test]
    fn mistyped_params_and_bad_inputs_are_errors() {
        let img = varied(3, 3);
        let input = || vec![("in".to_string(), NodeOutput::Image(img.clone()))];
        let nodes: [&dyn NodeImpl; 4] = [
            &GlareNode,
            &LensFlareNode,
            &ChromaticAberrationNode,
            &FilmGrainNode,
        ];
        let wrong_type = [
            p("threshold", ParamValue::Int(1)),
            p("decay", ParamValue::Bool(true)),
            p("center", ParamValue::Float(0.5)),
            p("seed", ParamValue::Float(1.0)),
        ];
        let non_finite = [
            p("intensity", ParamValue::Float(f32::INFINITY)),
            p("tint", ParamValue::Color([f32::NAN, 0.0, 0.0])),
            p("strength", ParamValue::Float(f32::NAN)),
            p("amount", ParamValue::Float(f32::INFINITY)),
        ];
        for ((node, wrong), nan) in nodes.iter().zip(wrong_type).zip(non_finite) {
            for param in [wrong, nan] {
                let got = node.eval(input(), std::slice::from_ref(&param), &ctx());
                assert!(
                    matches!(got, Err(EvalError::BadParam { .. })),
                    "{param:?} → {got:?}"
                );
            }
            assert!(matches!(
                node.eval(vec![], &[], &ctx()),
                Err(EvalError::MissingInput { .. })
            ));
            let uniform = vec![(
                "in".to_string(),
                NodeOutput::Uniform(ParamValue::Float(0.0)),
            )];
            assert!(matches!(
                node.eval(uniform, &[], &ctx()),
                Err(EvalError::TypeMismatch { .. })
            ));
        }
    }
}
