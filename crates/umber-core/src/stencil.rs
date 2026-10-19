//! The stencil engine's pure half (`docs/specs/stencil-engine-design.md`
//! "The design"): the screen-space transform of a stencil image and the
//! CPU rasterization of that transformed image into a paint gate.
//!
//! The gate [`rasterize_gate`] returns is exactly the shape
//! `umber_gpu::paint::PaintTarget::write_mask_region` takes: `w * h`
//! bytes, row-major, `255` = fully open, `0` = blocked, and (since the
//! soft-edges kill, 2026-10-19) intermediate bytes at stencil
//! boundaries = partial paint. No GPU, no egui, no I/O — the shell
//! (umber-app) owns the session state, the PNG load, the gate upload
//! and the overlay draw.
//!
//! Conventions (the judgment calls the next kill extends from):
//!
//! - **The forward map** (image px -> screen px) is
//!   `screen = center + R(rotation) * (image_px - image_size / 2) * scale`
//!   with `R(θ) = [[cos θ, -sin θ], [sin θ, cos θ]]`. Screen space is
//!   y-down, so a positive `rotation` turns the image CLOCKWISE on
//!   screen. [`forward_map`] is public so the overlay draw can place the
//!   quad's corners with the same math the gate uses; [`inverse_map`] is
//!   its exact inverse.
//! - **The image dims are an argument, not a transform field**: the same
//!   transform survives swapping the stencil image (the design's Reset is
//!   the only thing that re-derives it, via [`fit_to_viewport`]).
//! - **Texel centers sit at `i + 0.5`** in both spaces: target texel
//!   `(x, y)` samples at screen `(x + 0.5, y + 0.5)`, image texel `(i, j)`
//!   is centered at image px `(i + 0.5, j + 0.5)`.
//! - **v1 boundary: target texel space IS the transform's screen space.**
//!   [`rasterize_gate`] maps target texels straight through the
//!   transform; any viewport-px -> target-texel mapping is the shell's
//!   job before it builds the transform.
//! - **Bilinear with a zero border**: a tap outside the image reads `0`
//!   (the brief's "out-of-bounds sample = 0"). Coverage is soft at the
//!   image's bounds too: a sample at image px `x = 0` or `x = width`
//!   sits midway between a covered texel and the zero border and reads
//!   half coverage (`128`), not a hard edge.
//! - **Soft coverage (landed 2026-10-19, Open question 3)**: the gate
//!   carries the bilinear sample rounded to the nearest byte
//!   (`0..=255`) — no threshold in the rasterize path. Interior texels
//!   of a binary image sample their own texel value exactly (`0` or
//!   `255`), so binary images stay binary inside and soften only where
//!   a sample straddles an edge. The LOAD-time alpha cut
//!   ([`StencilImage::from_alpha`], `>= 128` -> `255` else `0`) is
//!   unchanged: the softness comes from sampling, not from the loader.
//! - **The map math runs in f64** and only the public API speaks f32:
//!   targets reach 8K px, where f32's ulp is ~5e-4, and the inverse's
//!   round trip is pinned to 1e-6.
//! - **Degenerate transforms never panic**: a non-finite or non-positive
//!   `scale`, or a non-finite center/rotation, rasterizes an all-zero
//!   (fully blocked) gate. [`inverse_map`] itself returns non-finite
//!   coordinates for a zero scale; callers that need a point check it.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The loader's alpha cut: an RGBA alpha byte `>= 128` is covered.
const ALPHA_CUT: u8 = 128;

/// Everything that can go wrong building a [`StencilImage`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StencilError {
    /// A zero width or height: there is no image to stencil through.
    #[error("empty stencil image ({width}x{height})")]
    EmptyImage {
        /// The requested width.
        width: u32,
        /// The requested height.
        height: u32,
    },
    /// `width * height * 4` doesn't fit in `usize`.
    #[error("stencil image {width}x{height} is too large")]
    TooLarge {
        /// The requested width.
        width: u32,
        /// The requested height.
        height: u32,
    },
    /// The RGBA byte count doesn't match `width * height * 4`.
    #[error("stencil RGBA byte count {actual} != expected {expected}")]
    ByteCount {
        /// `width * height * 4`.
        expected: usize,
        /// The length actually passed in.
        actual: usize,
    },
}

/// Where the stencil sits on screen: its image center lands at `center`
/// (screen px), scaled by `scale`, turned by `rotation` (radians,
/// clockwise on the y-down screen). See the module docs for the map.
///
/// `serde(default)` is container-level ON PURPOSE: a field-level default
/// would deserialize a missing `scale` as `f32::default()` = 0, a
/// degenerate transform; the container form fills gaps from
/// [`Default`] (scale 1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StencilTransform {
    /// Screen px the image's center maps to.
    pub center: [f32; 2],
    /// Screen px per image px.
    pub scale: f32,
    /// Radians; positive turns clockwise on the y-down screen.
    pub rotation: f32,
}

impl Default for StencilTransform {
    /// Center at the origin, unit scale, no rotation.
    fn default() -> Self {
        Self {
            center: [0.0, 0.0],
            scale: 1.0,
            rotation: 0.0,
        }
    }
}

/// A binary R8 coverage image: `255` = paint passes, `0` = blocked.
/// Built through [`Self::from_alpha`], which pins
/// `coverage.len() == width * height` (the fields are private so that
/// invariant holds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StencilImage {
    width: u32,
    height: u32,
    coverage: Vec<u8>,
}

impl StencilImage {
    /// Builds the coverage from tightly-packed RGBA8 bytes (the image
    /// loader's shape): alpha `>= 128` -> `255`, else `0`.
    ///
    /// # Errors
    ///
    /// [`StencilError::EmptyImage`] for a zero dimension,
    /// [`StencilError::TooLarge`] when `width * height * 4` overflows,
    /// [`StencilError::ByteCount`] when `rgba.len()` doesn't match it.
    pub fn from_alpha(width: u32, height: u32, rgba: &[u8]) -> Result<Self, StencilError> {
        if width == 0 || height == 0 {
            return Err(StencilError::EmptyImage { width, height });
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|texels| texels.checked_mul(4))
            .ok_or(StencilError::TooLarge { width, height })?;
        if rgba.len() != expected {
            return Err(StencilError::ByteCount {
                expected,
                actual: rgba.len(),
            });
        }
        let coverage = rgba
            .chunks_exact(4)
            .map(|px| if px[3] >= ALPHA_CUT { 255 } else { 0 })
            .collect();
        Ok(Self {
            width,
            height,
            coverage,
        })
    }

    /// Width in image px.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in image px.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The `width * height` coverage bytes, row-major.
    pub fn coverage(&self) -> &[u8] {
        &self.coverage
    }

    /// The coverage at texel `(x, y)`; `0` outside the image (the zero
    /// border).
    fn texel(&self, x: i64, y: i64) -> f64 {
        let in_x = (0..i64::from(self.width)).contains(&x);
        let in_y = (0..i64::from(self.height)).contains(&y);
        if !(in_x && in_y) {
            return 0.0;
        }
        // In range on both axes, so both casts are lossless.
        let index = y as usize * self.width as usize + x as usize;
        f64::from(self.coverage.get(index).copied().unwrap_or(0))
    }

    /// Bilinear coverage (0..=255) at image px `p`, texel centers at
    /// `i + 0.5`, zero border.
    fn sample(&self, [px, py]: [f64; 2]) -> f64 {
        // Beyond one texel past the bounds every tap is border; this
        // also rejects NaN/inf before the float -> int casts.
        let in_reach = (-1.0..=f64::from(self.width) + 1.0).contains(&px)
            && (-1.0..=f64::from(self.height) + 1.0).contains(&py);
        if !in_reach {
            return 0.0;
        }
        let (ux, uy) = (px - 0.5, py - 0.5);
        let (x0, y0) = (ux.floor(), uy.floor());
        let (fx, fy) = (ux - x0, uy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        self.texel(x0, y0) * (1.0 - fx) * (1.0 - fy)
            + self.texel(x0 + 1, y0) * fx * (1.0 - fy)
            + self.texel(x0, y0 + 1) * (1.0 - fx) * fy
            + self.texel(x0 + 1, y0 + 1) * fx * fy
    }
}

/// The transform's terms in f64, with the trig hoisted out of the
/// per-texel loop.
struct Mapping {
    center: [f64; 2],
    scale: f64,
    cos: f64,
    sin: f64,
    half: [f64; 2],
}

impl Mapping {
    fn new(transform: &StencilTransform, [image_w, image_h]: [u32; 2]) -> Self {
        let (sin, cos) = f64::from(transform.rotation).sin_cos();
        Self {
            center: widen(transform.center),
            scale: f64::from(transform.scale),
            cos,
            sin,
            half: [f64::from(image_w) * 0.5, f64::from(image_h) * 0.5],
        }
    }

    /// Image px -> screen px.
    fn forward(&self, [ix, iy]: [f64; 2]) -> [f64; 2] {
        let dx = (ix - self.half[0]) * self.scale;
        let dy = (iy - self.half[1]) * self.scale;
        let x = self.center[0] + self.cos * dx - self.sin * dy;
        let y = self.center[1] + self.sin * dx + self.cos * dy;
        [x, y]
    }

    /// Screen px -> image px: translate by -center, rotate by
    /// -rotation, divide by scale, then shift to the image's own px.
    fn inverse(&self, [sx, sy]: [f64; 2]) -> [f64; 2] {
        let dx = sx - self.center[0];
        let dy = sy - self.center[1];
        let rx = self.cos * dx + self.sin * dy;
        let ry = self.cos * dy - self.sin * dx;
        let x = rx / self.scale + self.half[0];
        let y = ry / self.scale + self.half[1];
        [x, y]
    }
}

fn widen([x, y]: [f32; 2]) -> [f64; 2] {
    [f64::from(x), f64::from(y)]
}

fn narrow([x, y]: [f64; 2]) -> [f32; 2] {
    [x as f32, y as f32]
}

/// Image px -> screen px under `transform`, for an image of
/// `image_size` = `[width, height]` (the overlay quad's corner math).
pub fn forward_map(
    transform: &StencilTransform,
    image_size: [u32; 2],
    image_point: [f32; 2],
) -> [f32; 2] {
    let mapping = Mapping::new(transform, image_size);
    narrow(mapping.forward(widen(image_point)))
}

/// Screen px -> image px under `transform`, for an image of
/// `image_size` = `[width, height]`: the exact inverse of
/// [`forward_map`]. A zero `scale` yields non-finite coordinates.
pub fn inverse_map(
    transform: &StencilTransform,
    image_size: [u32; 2],
    screen_point: [f32; 2],
) -> [f32; 2] {
    let mapping = Mapping::new(transform, image_size);
    narrow(mapping.inverse(widen(screen_point)))
}

/// Rasterizes `image` under `transform` into a `target_w * target_h`
/// gate (row-major, `write_mask_region`'s byte shape): each target
/// texel's center maps through [`inverse_map`], bilinear-samples the
/// coverage (zero border), and writes the sample rounded to the nearest
/// byte (`0..=255`; soft at boundaries since 2026-10-19). Interior
/// texels of a binary image write `0`/`255` exactly. A degenerate
/// transform (scale not finite and positive) blocks every texel.
pub fn rasterize_gate(
    image: &StencilImage,
    transform: &StencilTransform,
    target_w: u32,
    target_h: u32,
) -> Vec<u8> {
    let len = (target_w as usize).saturating_mul(target_h as usize);
    if !transform.scale.is_finite() || transform.scale <= 0.0 {
        return vec![0; len];
    }
    let mapping = Mapping::new(transform, [image.width, image.height]);
    let mut gate = Vec::with_capacity(len);
    for y in 0..target_h {
        for x in 0..target_w {
            let screen = [f64::from(x) + 0.5, f64::from(y) + 0.5];
            // The sample is a convex combination of 0..=255 taps, so the
            // rounded value is in range; the cast saturates regardless
            // (the never-crash gate).
            let coverage = image.sample(mapping.inverse(screen)).round();
            gate.push(coverage as u8);
        }
    }
    gate
}

/// The Reset transform: centered in a `view_w` x `view_h` viewport at
/// the largest scale that keeps the whole (unrotated) image inside it,
/// no rotation. A zero image dimension or a non-positive/non-finite
/// viewport falls back to scale 1 (nothing meaningful to fit).
///
/// The scale may exceed 1 (a small image fills the view) — the brief's
/// "largest scale keeping the image inside"; the design doc's chord
/// section words Reset as "scale 1", and the kill report names that
/// discrepancy for the shell kill to settle.
pub fn fit_to_viewport(image_w: u32, image_h: u32, view_w: f32, view_h: f32) -> StencilTransform {
    let usable = |v: f32| if v.is_finite() && v > 0.0 { v } else { 0.0 };
    let (view_w, view_h) = (usable(view_w), usable(view_h));
    let fits = image_w > 0 && image_h > 0 && view_w > 0.0 && view_h > 0.0;
    let scale = if fits {
        (view_w / image_w as f32).min(view_h / image_h as f32)
    } else {
        1.0
    };
    StencilTransform {
        center: [view_w * 0.5, view_h * 0.5],
        scale,
        rotation: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    /// A 4x4 image from 0/1 rows (1 = alpha 255, 0 = alpha 0).
    fn image_4x4(rows: [[u8; 4]; 4]) -> StencilImage {
        let rgba: Vec<u8> = rows
            .iter()
            .flatten()
            .flat_map(|&on| [0, 0, 0, on * 255])
            .collect();
        StencilImage::from_alpha(4, 4, &rgba).unwrap()
    }

    /// 0/1 rows -> the 0/255 gate bytes.
    fn bytes<const W: usize>(rows: &[[u8; W]]) -> Vec<u8> {
        rows.iter().flatten().map(|&on| on * 255).collect()
    }

    /// Both coordinates within 1e-6.
    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() <= 1e-6 && (a[1] - b[1]).abs() <= 1e-6
    }

    /// Deliberately asymmetric under every rotation and flip.
    const PATTERN: [[u8; 4]; 4] = [[1, 0, 0, 0], [1, 1, 0, 0], [0, 1, 1, 0], [0, 0, 0, 1]];

    /// Center on the image's own center, unit scale, no turn: image px
    /// == screen px.
    const IDENTITY_4X4: StencilTransform = StencilTransform {
        center: [2.0, 2.0],
        scale: 1.0,
        rotation: 0.0,
    };

    #[test]
    fn default_transform_is_origin_unit_unrotated() {
        let t = StencilTransform::default();
        assert_eq!(t.center, [0.0, 0.0]);
        assert_eq!(t.scale, 1.0);
        assert_eq!(t.rotation, 0.0);
    }

    #[test]
    fn serde_missing_fields_fill_from_default_not_zero() {
        // A field-level default would give scale 0 here.
        let json = r#"{"center":[3.0,4.0]}"#;
        let t: StencilTransform = serde_json::from_str(json).unwrap();
        assert_eq!(
            t,
            StencilTransform {
                center: [3.0, 4.0],
                scale: 1.0,
                rotation: 0.0,
            }
        );
        let json = serde_json::to_string(&t).unwrap();
        let back: StencilTransform = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn forward_and_inverse_round_trip_within_1e6() {
        // Small magnitudes (|screen| < 8, |image px| < 4): the f32
        // ulp there is <= 4.8e-7, so the two output roundings (scaled by
        // at most 2 or 1/0.75) stay under 1e-6 and the bound still
        // catches any real error in the map math.
        let transforms = [
            StencilTransform {
                center: [2.5, 1.5],
                scale: 1.0,
                rotation: 0.0,
            },
            StencilTransform {
                center: [2.5, 1.5],
                scale: 2.0,
                rotation: 0.3,
            },
            StencilTransform {
                center: [-1.25, 3.0],
                scale: 0.75,
                rotation: -1.2,
            },
            StencilTransform {
                center: [2.0, 2.0],
                scale: 1.5,
                rotation: FRAC_PI_2,
            },
        ];
        for t in &transforms {
            for j in 0..8 {
                for i in 0..8 {
                    // Interior image points 0.25..=3.75.
                    let p = [0.25 + i as f32 * 0.5, 0.25 + j as f32 * 0.5];
                    let s = forward_map(t, [4, 4], p);
                    let back = inverse_map(t, [4, 4], s);
                    assert!(close(back, p), "{t:?} {p:?} -> {back:?}");
                    let again = forward_map(t, [4, 4], back);
                    assert!(close(again, s), "{t:?} {s:?} -> {again:?}");
                }
            }
        }
    }

    #[test]
    fn forward_map_places_the_image_center_and_turns_clockwise() {
        let t = StencilTransform {
            center: [10.0, 20.0],
            scale: 2.0,
            rotation: FRAC_PI_2,
        };
        // The image center lands on `center`.
        assert_eq!(forward_map(&t, [4, 4], [2.0, 2.0]), [10.0, 20.0]);
        // Image +x (one px right of center) -> R(90°)·(1,0)·2 = (0, 2):
        // screen DOWN, i.e. clockwise on the y-down screen.
        let right = forward_map(&t, [4, 4], [3.0, 2.0]);
        assert!(close(right, [10.0, 22.0]), "{right:?}");
    }

    #[test]
    fn identity_transform_reproduces_the_image_bytes() {
        // Target texel (x, y) samples screen (x+.5, y+.5) = image px
        // (x+.5, y+.5): u = x exactly, fx = 0 -> the texel itself.
        let image = image_4x4(PATTERN);
        let gate = rasterize_gate(&image, &IDENTITY_4X4, 4, 4);
        assert_eq!(gate, bytes(&PATTERN));
        assert_eq!(gate, image.coverage());
    }

    #[test]
    fn two_x_scale_doubles_each_texel_and_softens_the_ring() {
        // Target 10x10, center (5,5), scale 2: the 8x8 footprint spans
        // screen [1, 9). Texel x samples image px p = (x+.5-5)/2 + 2 =
        // x/2 - 0.25, so u = x/2 - 0.75 and the column weights are:
        //   x=0: col -1 .75 / col 0 .25   x=1: col -1 .25 / col 0 .75
        //   x=2: col 0 .75 / col 1 .25    x=3: col 0 .25 / col 1 .75
        //   x=4: col 1 .75 / col 2 .25    x=5: col 1 .25 / col 2 .75
        //   x=6: col 2 .75 / col 3 .25    x=7: col 2 .25 / col 3 .75
        //   x=8: col 3 .75 / border .25   x=9: col 3 .25 / border .75
        // and identically down the rows (2D weights are the products).
        // Texels whose taps all read 0 stay exactly 0; a sample with all
        // four taps covered would read exactly 255, but PATTERN has no
        // 2x2 all-covered block and every sample here sits a quarter
        // texel off the centers, so the brightest byte is 239. Every
        // texel straddling an edge gives the partial byte, rounded to
        // nearest. Hand examples: (0,0) = .25*.25*255 = 15.9375 ->
        // 16; (1,0) = .75*.25*255 = 47.8125 -> 48; (2,2) =
        // (.75*.75 + .25*.75 + .25*.25)*255 = .8125*255 = 207.1875 ->
        // 207; (2,3) = (.25*.75 + .75*.75 + .75*.25)*255 = .9375*255 =
        // 239.0625 -> 239.
        let image = image_4x4(PATTERN);
        let t = StencilTransform {
            center: [5.0, 5.0],
            scale: 2.0,
            rotation: 0.0,
        };
        let expected = vec![
            16, 48, 48, 16, 0, 0, 0, 0, 0, 0, //
            48, 143, 143, 48, 0, 0, 0, 0, 0, 0, //
            64, 191, 207, 112, 48, 16, 0, 0, 0, 0, //
            64, 191, 239, 207, 143, 48, 0, 0, 0, 0, //
            48, 143, 207, 239, 207, 112, 48, 16, 0, 0, //
            16, 48, 112, 207, 239, 207, 143, 48, 0, 0, //
            0, 0, 48, 143, 191, 191, 159, 96, 48, 16, //
            0, 0, 16, 48, 64, 64, 96, 159, 143, 48, //
            0, 0, 0, 0, 0, 0, 48, 143, 143, 48, //
            0, 0, 0, 0, 0, 0, 16, 48, 48, 16, //
        ];
        assert_eq!(rasterize_gate(&image, &t, 10, 10), expected);
    }

    #[test]
    fn quarter_turn_permutes_as_pinned() {
        // Derived by hand: rotation θ = π/2, center (2,2), scale 1.
        // inverse: d = s - c; R(-θ)·d = (cos·dx + sin·dy, -sin·dx +
        // cos·dy) = (dy, -dx). Texel (x, y): d = (x-1.5, y-1.5) ->
        // p = (y-1.5+2, -(x-1.5)+2) = (y+.5, (3-x)+.5), the center of
        // image texel (col y, row 3-x). So out[y][x] = img[3-x][y]:
        //   y=0: img[3][0] img[2][0] img[1][0] img[0][0] = 0 0 1 1
        //   y=1: img[3][1] img[2][1] img[1][1] img[0][1] = 0 1 1 0
        //   y=2: img[3][2] img[2][2] img[1][2] img[0][2] = 0 1 0 0
        //   y=3: img[3][3] img[2][3] img[1][3] img[0][3] = 1 0 0 0
        // (Cross-check: a clockwise turn's top row is the old left
        // column read bottom-up: 0 0 1 1.) f32 cos(π/2) ≈ -4.4e-8, so
        // samples sit ~1e-7 off the centers; the dominant weight stays
        // ~1 (the residual coverage is ~5e-5 of a byte), so every sample
        // still rounds to its own 0/255.
        let image = image_4x4(PATTERN);
        let t = StencilTransform {
            rotation: FRAC_PI_2,
            ..IDENTITY_4X4
        };
        let expected = bytes(&[[0, 0, 1, 1], [0, 1, 1, 0], [0, 1, 0, 0], [1, 0, 0, 0]]);
        assert_eq!(rasterize_gate(&image, &t, 4, 4), expected);
    }

    #[test]
    fn texels_outside_the_coverage_are_zero() {
        // A fully covered 4x4 at identity in a 6x6 target: columns and
        // rows 4..6 sample image px 4.5 / 5.5 -> every tap is border.
        let image = image_4x4([[1; 4]; 4]);
        let expected = bytes(&[
            [1, 1, 1, 1, 0, 0],
            [1, 1, 1, 1, 0, 0],
            [1, 1, 1, 1, 0, 0],
            [1, 1, 1, 1, 0, 0],
            [0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0],
        ]);
        assert_eq!(rasterize_gate(&image, &IDENTITY_4X4, 6, 6), expected);
        // Moved far off-target: nothing opens.
        let far = StencilTransform {
            center: [1000.0, -1000.0],
            ..IDENTITY_4X4
        };
        assert_eq!(rasterize_gate(&image, &far, 6, 6), vec![0; 36]);
    }

    #[test]
    fn half_texel_boundary_samples_read_the_soft_coverage() {
        // Image 2x1 = [covered, empty]; target texel (0,0)'s center is
        // image px (1.0, 0.5) when the transform's center is (0.5, 0.5):
        // u = p - 0.5 = (0.5, 0) -> half of texel 0 + half of texel 1 =
        // 127.5 of 255, which rounds to 128 (ties away from zero). This
        // is the intermediate byte the soft gate writes; there is no
        // threshold left in the rasterize path.
        let rgba = [0, 0, 0, 255, 0, 0, 0, 0];
        let image = StencilImage::from_alpha(2, 1, &rgba).unwrap();
        let t = StencilTransform {
            center: [0.5, 0.5],
            scale: 1.0,
            rotation: 0.0,
        };
        assert_eq!(rasterize_gate(&image, &t, 1, 1), vec![128]);
        // A quarter texel toward the empty side: p = (1.25, 0.5), u =
        // (0.75, 0) -> texel 0 at weight 0.25 = 63.75 -> 64.
        let t_low = StencilTransform {
            center: [0.25, 0.5],
            ..t
        };
        assert_eq!(rasterize_gate(&image, &t_low, 1, 1), vec![64]);
        // A quarter texel toward the covered side: p = (0.75, 0.5), u =
        // (0.25, 0) -> texel 0 at weight 0.75 = 191.25 -> 191.
        let t_high = StencilTransform {
            center: [0.75, 0.5],
            ..t
        };
        assert_eq!(rasterize_gate(&image, &t_high, 1, 1), vec![191]);
        // Both taps fully on one side stay exactly binary (regression:
        // softness is a boundary effect, not a global scale). Center 1.0
        // puts the sample at image px 0.5 (u = 0, texel 0 exactly).
        let covered = StencilTransform {
            center: [1.0, 0.5],
            ..t
        };
        assert_eq!(rasterize_gate(&image, &covered, 1, 1), vec![255]);
        // Center 0.0 puts it at image px 1.5 (u = 1.0, texel 1 exactly).
        let empty = StencilTransform {
            center: [0.0, 0.5],
            ..t
        };
        assert_eq!(rasterize_gate(&image, &empty, 1, 1), vec![0]);
    }

    #[test]
    fn a_degenerate_scale_blocks_the_whole_gate() {
        let image = image_4x4([[1; 4]; 4]);
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let t = StencilTransform {
                scale,
                ..IDENTITY_4X4
            };
            let gate = rasterize_gate(&image, &t, 4, 3);
            assert_eq!(gate, vec![0; 12], "scale {scale}");
        }
        let nan_center = StencilTransform {
            center: [f32::NAN, 2.0],
            ..IDENTITY_4X4
        };
        assert_eq!(rasterize_gate(&image, &nan_center, 4, 4), vec![0; 16]);
        assert!(rasterize_gate(&image, &IDENTITY_4X4, 0, 7).is_empty());
    }

    #[test]
    fn from_alpha_cuts_at_exactly_128() {
        // RGB at 200 (>= 128): a loader reading the wrong channel would
        // open every texel.
        let rgba: Vec<u8> = [127, 128, 0, 255]
            .iter()
            .flat_map(|&a| [200, 200, 200, a])
            .collect();
        let image = StencilImage::from_alpha(2, 2, &rgba).unwrap();
        assert_eq!((image.width(), image.height()), (2, 2));
        assert_eq!(image.coverage(), &[0, 255, 0, 255]);
        // 127 itself is the last closed value; 128 the first open one.
    }

    #[test]
    fn from_alpha_rejects_empty_and_miscounted_input() {
        assert_eq!(
            StencilImage::from_alpha(0, 4, &[]),
            Err(StencilError::EmptyImage {
                width: 0,
                height: 4,
            })
        );
        assert_eq!(
            StencilImage::from_alpha(3, 0, &[]),
            Err(StencilError::EmptyImage {
                width: 3,
                height: 0,
            })
        );
        assert_eq!(
            StencilImage::from_alpha(2, 2, &[0; 15]),
            Err(StencilError::ByteCount {
                expected: 16,
                actual: 15,
            })
        );
        // RGB (3 bytes/px) passed where RGBA is required.
        assert_eq!(
            StencilImage::from_alpha(2, 2, &[0; 12]),
            Err(StencilError::ByteCount {
                expected: 16,
                actual: 12,
            })
        );
    }

    #[test]
    fn fit_to_viewport_takes_the_limiting_axis_and_centers() {
        // 400x100 in 200x300: min(200/400, 300/100) = 0.5 (width
        // limits); center (100, 150).
        let t = fit_to_viewport(400, 100, 200.0, 300.0);
        assert_eq!(
            t,
            StencilTransform {
                center: [100.0, 150.0],
                scale: 0.5,
                rotation: 0.0,
            }
        );
        // The image's left edge touches the viewport's: corner (0,0) ->
        // (100,150) + (-200,-50)*0.5 = (0, 125).
        assert_eq!(forward_map(&t, [400, 100], [0.0, 0.0]), [0.0, 125.0]);
        // A small image scales UP to fill: 10x20 in 100x100 -> min(10, 5).
        assert_eq!(fit_to_viewport(10, 20, 100.0, 100.0).scale, 5.0);
    }

    #[test]
    fn fit_to_viewport_degenerate_input_falls_back_to_unit_scale() {
        let t = fit_to_viewport(0, 10, 100.0, 50.0);
        assert_eq!(t.scale, 1.0);
        assert_eq!(t.center, [50.0, 25.0]);
        let t = fit_to_viewport(10, 10, 0.0, f32::NAN);
        assert_eq!(t.scale, 1.0);
        assert_eq!(t.center, [0.0, 0.0]);
    }
}
