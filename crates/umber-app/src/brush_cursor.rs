//! The in-viewport brush cursor (GUI plan G1 `g1-brush-cursor`,
//! `docs/specs/gui-plan-2026-10-15.md`; sources: Painter C1, Blender
//! C4.1, photocraft #1 in `docs/research/gui-research-*-2026-10-15.md`).
//!
//! Painting happened blind until now: the OS arrow sat over the mesh and
//! nothing showed where the dab would land or how big it was. This draws
//! a ring at the brush's on-screen radius under the pointer, an inner
//! ring where the hardness core ends (full coverage inside it), and hides
//! the OS cursor while the ring shows.
//!
//! # The on-screen radius
//!
//! The brush radius is in TEXELS. On screen it depends on how many texels
//! the surface under the pointer covers per pixel, which this measures
//! directly: the pointer and a point `PROBE_PX` pixels to its right are
//! both cast onto the mesh. The UV distance between the two hits, times
//! the target's texels-per-UV, is the texel count those pixels span. So
//! `radius_px = radius_texels * PROBE_PX / texels_spanned`. That folds the
//! camera distance, perspective, the surface's tilt and the UV layout's
//! local density into one measured ratio, with no Jacobian to derive.
//!
//! Both probes run (8 px right and 8 px left). A probe that crosses a UV
//! seam reads a huge UV jump for a tiny screen step. There is no absolute
//! texels-per-pixel threshold for that: a far-away 4K mesh legitimately
//! squeezes hundreds of texels into a pixel. So the two sides are
//! compared: when one span is more than [`SEAM_RATIO`] times the other,
//! the larger is the seam crossing and the smaller is used. One side
//! missing (a silhouette edge) uses the other. Both failing gives a
//! crosshair.
//!
//! # Judgment calls
//!
//! - **OS bitmap cursor where the platform allows it** (photocraft #1):
//!   [`bitmap`] rasterizes the ring into an egui `CustomCursorImage` handed
//!   to the OS via `Context::set_cursor_image`, so the outline follows the
//!   hardware pointer with no render lag. The painted ring ([`paint`]) is
//!   the fallback where bitmaps fail: web, any scale != 1x (Wayland
//!   fractional included — the hotspot would misplace), and tips larger
//!   than [`MAX_BITMAP_SIDE_PX`]. The shell clears the image at the start
//!   of every pass, so outside the viewport the normal cursor returns.
//! - **Fixed contrast colours.** The ring is black-under-white (two
//!   strokes), readable on any paint colour, and independent of the theme.
//! - **Shown only while the pointer is over the mesh and a paint tool is
//!   active.** Orbiting with the pointer off the mesh keeps the arrow.

use egui::{Color32, Pos2, Stroke};

/// Below this on-screen radius the ring is unreadable, so the cursor is a
/// crosshair instead (photocraft's `MIN_RADIUS`).
pub const MIN_RING_PX: f32 = 3.0;
/// The probe step for the screen-to-texel ratio, in pixels.
pub const PROBE_PX: f32 = 8.0;
/// Two probe spans differing by more than this factor mean one of them
/// crossed a UV seam (the surface's texel density cannot change that much
/// across 16 px of continuous surface).
pub const SEAM_RATIO: f32 = 4.0;
/// The largest ring drawn, px: a huge brush close to the camera would
/// otherwise draw a ring far outside the viewport.
pub const MAX_RING_PX: f32 = 4096.0;
/// Crosshair arm length, px.
pub const CROSSHAIR_PX: f32 = 6.0;
/// The largest bitmap side handed to the OS, px. Linux HW-cursor planes
/// cap near 255; keep a margin. Oversize tips fall back to [`paint`].
pub const MAX_BITMAP_SIDE_PX: u16 = 128;
/// Transparent margin around the ring inside its bitmap, device px (the
/// 3 px dark outline needs room).
const BITMAP_MARGIN_PX: f32 = 4.0;

/// One `[` / `]` press scales the brush size by this factor (Painter
/// steps the size multiplicatively, so small brushes step finely).
pub const SIZE_STEP: f32 = 1.1;
/// Ctrl+RMB drag: pixels of horizontal drag that double the size.
pub const RESIZE_PX_PER_DOUBLING: f32 = 150.0;
/// Ctrl+RMB drag: pixels of vertical drag that sweep the full hardness
/// range (up = harder).
pub const HARDNESS_PX_PER_FULL: f32 = 200.0;

/// One `[` (`grow = false`) or `]` (`grow = true`) step from `size`.
/// Callers clamp through `PaintState::set_brush_size`.
pub fn step_size(size: f32, grow: bool) -> f32 {
    if grow {
        size * SIZE_STEP
    } else {
        size / SIZE_STEP
    }
}

/// The size and hardness after a Ctrl+RMB drag of `(dx, dy)` pixels from
/// the drag's `start` values. Horizontal is exponential in size (right =
/// bigger, 150 px doubles it). Vertical is linear in hardness (up =
/// harder; screen y grows downward, so `-dy`). The result is relative to
/// the drag origin, not accumulated per frame, so a drag returned to its
/// origin restores the exact start values.
pub fn resize_drag(start: (f32, f32), total_delta: egui::Vec2) -> (f32, f32) {
    let (size0, hard0) = start;
    let size = size0 * 2f32.powf(total_delta.x / RESIZE_PX_PER_DOUBLING);
    let hardness = (hard0 - total_delta.y / HARDNESS_PX_PER_FULL).clamp(0.0, 1.0);
    (size, hardness)
}

/// What the viewport draws at the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorShape {
    /// The brush outline: `radius_px` and the hardness-core radius.
    Ring {
        /// The brush radius on screen.
        radius_px: f32,
        /// The radius of the full-coverage core (`hardness * radius_px`);
        /// `None` when the core would be indistinguishable from the rim
        /// or from the centre.
        core_px: Option<f32>,
    },
    /// A crosshair: the brush is too small on screen to outline, or the
    /// probe could not measure it.
    Crosshair,
}

/// Texels one probe step spans, from the two probe hits' UVs. `None` for a
/// degenerate span (zero or non-finite).
pub fn texels_spanned(uv_a: [f32; 2], uv_b: [f32; 2], texels_per_uv: f32) -> Option<f32> {
    let du = uv_b[0] - uv_a[0];
    let dv = uv_b[1] - uv_a[1];
    let texels = (du * du + dv * dv).sqrt() * texels_per_uv;
    (texels.is_finite() && texels > 0.0).then_some(texels)
}

/// The surface span from the right and left probes: either alone when the
/// other missed; the smaller when they disagree by more than
/// [`SEAM_RATIO`] (the larger crossed a seam); their mean otherwise.
pub fn combine_spans(right: Option<f32>, left: Option<f32>) -> Option<f32> {
    match (right, left) {
        (Some(r), Some(l)) => {
            let (lo, hi) = if r < l { (r, l) } else { (l, r) };
            Some(if hi > lo * SEAM_RATIO {
                lo
            } else {
                (r + l) * 0.5
            })
        }
        (one, None) | (None, one) => one,
    }
}

/// The brush's on-screen radius from a measured probe span.
pub fn screen_radius(radius_texels: f32, texels_spanned: f32) -> f32 {
    (radius_texels * PROBE_PX / texels_spanned).min(MAX_RING_PX)
}

/// The cursor shape for a measured on-screen radius and the brush
/// hardness. `radius_px = None` means the probe failed.
pub fn shape(radius_px: Option<f32>, hardness: f32) -> CursorShape {
    match radius_px {
        Some(r) if r.is_finite() && r >= MIN_RING_PX => {
            let h = hardness.clamp(0.0, 1.0);
            let core = r * h;
            // A core within 2 px of the rim or of the centre adds a ring
            // that reads as noise.
            let core_px = (core > 2.0 && r - core > 2.0).then_some(core);
            CursorShape::Ring {
                radius_px: r,
                core_px,
            }
        }
        _ => CursorShape::Crosshair,
    }
}

/// Draws `shape` at `at`. Two-pass strokes (dark under light) so the ring
/// reads on any paint colour.
pub fn paint(painter: &egui::Painter, at: Pos2, shape: CursorShape) {
    let dark = Stroke::new(3.0, Color32::from_black_alpha(160));
    let light = Stroke::new(1.0, Color32::from_white_alpha(230));
    match shape {
        CursorShape::Ring { radius_px, core_px } => {
            painter.circle_stroke(at, radius_px, dark);
            painter.circle_stroke(at, radius_px, light);
            if let Some(core) = core_px {
                let faint = Stroke::new(1.0, Color32::from_white_alpha(90));
                painter.circle_stroke(at, core, faint);
            }
            // A centre dot: the exact dab position.
            painter.circle_filled(at, 1.5, Color32::from_white_alpha(230));
        }
        CursorShape::Crosshair => {
            for stroke in [dark, light] {
                painter.line_segment(
                    [
                        at - egui::vec2(CROSSHAIR_PX, 0.0),
                        at + egui::vec2(CROSSHAIR_PX, 0.0),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        at - egui::vec2(0.0, CROSSHAIR_PX),
                        at + egui::vec2(0.0, CROSSHAIR_PX),
                    ],
                    stroke,
                );
            }
        }
    }
}

/// Whether the OS-bitmap cursor path is usable at this device scale. Web
/// has no bitmap upload; a scale != 1x (Wayland fractional, HiDPI) would
/// misplace the hotspot, so both fall back to the painted ring.
/// Conservative on purpose.
pub fn bitmap_supported(pixels_per_point: f32) -> bool {
    !cfg!(target_arch = "wasm32") && pixels_per_point == 1.0
}

/// Rasterizes `shape` into an OS cursor bitmap at device scale
/// `pixels_per_point`. `None` when the path is unsupported (web, scale
/// != 1x) or the tip is too large for a cursor plane — the caller then
/// paints the fallback ring. The hotspot is the bitmap centre (the dab
/// position).
pub fn bitmap(shape: CursorShape, pixels_per_point: f32) -> Option<egui::CustomCursorImage> {
    if !bitmap_supported(pixels_per_point) {
        return None;
    }
    let (side, rgba) = match shape {
        CursorShape::Ring { radius_px, core_px } => {
            rasterize_ring(radius_px, core_px, pixels_per_point)?
        }
        CursorShape::Crosshair => rasterize_crosshair(pixels_per_point),
    };
    let hot = side / 2;
    Some(egui::CustomCursorImage {
        rgba: std::sync::Arc::from(rgba.into_boxed_slice()),
        size: [side, side],
        hotspot: [hot, hot],
    })
}

/// Pure rasterizer for the ring: the same two-pass look as [`paint`]
/// (3 px dark outline, 1 px light line on the same radius, faint core
/// ring, centre dot) over transparent, at device scale `scale`.
/// Returns `(side, rgba bytes)`; `None` when the bitmap would exceed
/// [`MAX_BITMAP_SIDE_PX`].
fn rasterize_ring(radius_px: f32, core_px: Option<f32>, scale: f32) -> Option<(u16, Vec<u8>)> {
    let r = radius_px * scale;
    let side_f = 2.0 * (r + BITMAP_MARGIN_PX);
    if !(side_f.is_finite()) || side_f > MAX_BITMAP_SIDE_PX as f32 {
        return None;
    }
    let side = side_f.ceil().max(3.0) as u16;
    let c = side as f32 / 2.0;
    let mut rgba = vec![0u8; (side as usize) * (side as usize) * 4];
    for y in 0..side as usize {
        for x in 0..side as usize {
            let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
            let i = (y * side as usize + x) * 4;
            // Dark outline: |d - r| <= 1.5*scale.
            if (d - r).abs() <= 1.5 * scale {
                rgba[i..i + 4].copy_from_slice(&[0, 0, 0, 160]);
            }
            // Faint core ring (50 % coverage): |d - core| <= 0.5*scale.
            if let Some(core) = core_px {
                if (d - core * scale).abs() <= 0.5 * scale {
                    rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 90]);
                }
            }
            // Light line on the rim: |d - r| <= 0.75*scale (over the dark).
            // 0.75 not 0.5 so the pixel just outside r (d = r + 0.51 for
            // even-side bitmaps) still lands on the light line.
            if (d - r).abs() <= 0.75 * scale {
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 230]);
            }
            // Centre dot.
            if d <= 1.5 * scale {
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 230]);
            }
        }
    }
    Some((side, rgba))
}

/// Pure rasterizer for the crosshair bitmap: fixed side, dark-under-light
/// arms, transparent elsewhere. Hotspot = centre.
fn rasterize_crosshair(scale: f32) -> (u16, Vec<u8>) {
    let arm = (CROSSHAIR_PX * scale).ceil().max(1.0);
    let side = (2.0 * arm + 3.0 * scale).ceil().max(7.0) as u16;
    let c = side as f32 / 2.0;
    let mut rgba = vec![0u8; (side as usize) * (side as usize) * 4];
    for y in 0..side as usize {
        for x in 0..side as usize {
            let dx = (x as f32 + 0.5 - c).abs();
            let dy = (y as f32 + 0.5 - c).abs();
            let on_arm = dx <= arm && dy <= 1.5 * scale || dy <= arm && dx <= 1.5 * scale;
            if !on_arm {
                continue;
            }
            let i = (y * side as usize + x) * 4;
            let on_core = dx <= arm && dy <= 0.5 * scale || dy <= arm && dx <= 0.5 * scale;
            if on_core {
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 230]);
            } else {
                rgba[i..i + 4].copy_from_slice(&[0, 0, 0, 160]);
            }
        }
    }
    (side, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_support_needs_scale_1x_and_native() {
        assert!(bitmap_supported(1.0) != cfg!(target_arch = "wasm32"));
        assert!(!bitmap_supported(2.0), "HiDPI/Wayland fractional: painted");
        assert!(!bitmap_supported(1.5));
    }

    #[test]
    fn ring_bitmap_caps_at_max_side_and_centres_the_hotspot() {
        // Oversize tip: no bitmap, the painted fallback takes over.
        assert!(rasterize_ring(200.0, None, 1.0).is_none());
        assert!(bitmap(
            CursorShape::Ring {
                radius_px: 200.0,
                core_px: None
            },
            1.0
        )
        .is_none());
        let (side, rgba) = rasterize_ring(10.0, Some(5.0), 1.0).unwrap();
        assert_eq!(side as f32, (2.0 * (10.0 + BITMAP_MARGIN_PX)).ceil());
        assert_eq!(rgba.len(), (side as usize) * (side as usize) * 4);
        let c = side as usize / 2;
        // Centre dot is opaque; the corner is transparent.
        assert_eq!(
            &rgba[(c * side as usize + c) * 4..][..4],
            &[255, 255, 255, 230]
        );
        assert_eq!(&rgba[..4], &[0, 0, 0, 0]);
        // A pixel on the rim (d == r) is the light line.
        let rim = ((c + 10) * side as usize + c) * 4;
        assert_eq!(&rgba[rim..rim + 4], &[255, 255, 255, 230]);
        let img = bitmap(
            CursorShape::Ring {
                radius_px: 10.0,
                core_px: None,
            },
            1.0,
        )
        .unwrap();
        assert_eq!(img.size, [side, side]);
        assert_eq!(img.hotspot, [side / 2, side / 2]);
        // Scale 2x is unsupported -> painted fallback.
        assert!(bitmap(CursorShape::Crosshair, 2.0).is_none());
    }

    #[test]
    fn crosshair_bitmap_has_opaque_centre_and_transparent_corners() {
        let (side, rgba) = rasterize_crosshair(1.0);
        let c = side as usize / 2;
        assert_eq!(
            &rgba[(c * side as usize + c) * 4..][..4],
            &[255, 255, 255, 230]
        );
        assert_eq!(&rgba[..4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[rgba.len() - 4..], &[0, 0, 0, 0]);
    }

    #[test]
    fn size_steps_are_multiplicative_and_reversible() {
        let up = step_size(1.0, true);
        assert!((up - SIZE_STEP).abs() < 1e-6);
        assert!((step_size(up, false) - 1.0).abs() < 1e-6);
        // Ten presses: ~2.59x.
        let ten = (0..10).fold(1.0, |s, _| step_size(s, true));
        assert!((ten - SIZE_STEP.powi(10)).abs() < 1e-4, "{ten}");
    }

    #[test]
    fn resize_drag_maps_axes_and_returns_home() {
        let start = (1.0, 0.5);
        // 150 px right doubles; 150 px left halves.
        let (s, h) = resize_drag(start, egui::vec2(RESIZE_PX_PER_DOUBLING, 0.0));
        assert!((s - 2.0).abs() < 1e-5 && (h - 0.5).abs() < 1e-6);
        let (s, _) = resize_drag(start, egui::vec2(-RESIZE_PX_PER_DOUBLING, 0.0));
        assert!((s - 0.5).abs() < 1e-5);
        // Up = harder, down = softer, clamped.
        let (_, h) = resize_drag(start, egui::vec2(0.0, -HARDNESS_PX_PER_FULL / 4.0));
        assert!((h - 0.75).abs() < 1e-6, "{h}");
        let (_, h) = resize_drag(start, egui::vec2(0.0, 10_000.0));
        assert_eq!(h, 0.0);
        // Back at the origin: exactly the start values.
        assert_eq!(resize_drag(start, egui::Vec2::ZERO), start);
    }

    #[test]
    fn span_measures_texels_and_rejects_degenerate_probes() {
        // 8 px probe crossing 1/64 of UV on a 512-texel target = 8 texels.
        let t = texels_spanned([0.5, 0.5], [0.5 + 1.0 / 64.0, 0.5], 512.0).unwrap();
        assert!((t - 8.0).abs() < 1e-4, "{t}");
        // Diagonal: Euclidean in UV.
        let d = texels_spanned([0.0, 0.0], [0.03, 0.04], 100.0).unwrap();
        assert!((d - 5.0).abs() < 1e-4, "{d}");
        // Same UV twice (a degenerate face): no measurement.
        assert_eq!(texels_spanned([0.2, 0.2], [0.2, 0.2], 512.0), None);
        assert_eq!(texels_spanned([0.0, 0.0], [f32::NAN, 0.0], 512.0), None);
        // A far 4K mesh: 2048 texels in 8 px is a real, measurable span
        // (no absolute threshold may reject it).
        assert_eq!(texels_spanned([0.1, 0.1], [0.6, 0.1], 4096.0), Some(2048.0));
    }

    #[test]
    fn combine_spans_drops_the_seam_side() {
        // Agreeing sides: the mean.
        assert_eq!(combine_spans(Some(8.0), Some(10.0)), Some(9.0));
        // One side crossed a seam (100x): the smaller is the surface.
        assert_eq!(combine_spans(Some(800.0), Some(8.0)), Some(8.0));
        assert_eq!(combine_spans(Some(8.0), Some(800.0)), Some(8.0));
        // Exactly at the ratio is still agreement.
        assert_eq!(combine_spans(Some(4.0), Some(16.0)), Some(10.0));
        // Silhouette: one side missed.
        assert_eq!(combine_spans(None, Some(8.0)), Some(8.0));
        assert_eq!(combine_spans(Some(8.0), None), Some(8.0));
        assert_eq!(combine_spans(None, None), None);
    }

    #[test]
    fn radius_is_texels_over_density() {
        // 1 texel per px -> a 24-texel brush is 24 px.
        assert!((screen_radius(24.0, PROBE_PX) - 24.0).abs() < 1e-5);
        // Zoomed in to 4 px per texel (the probe spans 2 texels).
        assert!((screen_radius(24.0, 2.0) - 96.0).abs() < 1e-4);
        // Far away: 4 texels per px.
        assert!((screen_radius(24.0, 32.0) - 6.0).abs() < 1e-5);
        // Absurdly close: clamped.
        assert_eq!(screen_radius(24.0, 1e-6), MAX_RING_PX);
    }

    #[test]
    fn small_or_unmeasured_brush_is_a_crosshair() {
        assert_eq!(shape(None, 0.5), CursorShape::Crosshair);
        assert_eq!(shape(Some(MIN_RING_PX - 0.1), 0.5), CursorShape::Crosshair);
        assert_eq!(shape(Some(f32::NAN), 0.5), CursorShape::Crosshair);
        assert!(matches!(
            shape(Some(MIN_RING_PX), 0.5),
            CursorShape::Ring { .. }
        ));
    }

    #[test]
    fn core_ring_only_when_distinct() {
        // 40 px, hardness 0.5: a 20 px core.
        assert_eq!(
            shape(Some(40.0), 0.5),
            CursorShape::Ring {
                radius_px: 40.0,
                core_px: Some(20.0)
            }
        );
        // Hardness 1 (core == rim) and 0 (no core): rim only.
        for h in [1.0, 0.0, 0.99] {
            assert_eq!(
                shape(Some(40.0), h),
                CursorShape::Ring {
                    radius_px: 40.0,
                    core_px: None
                },
                "hardness {h}"
            );
        }
        // Out-of-range hardness clamps.
        assert_eq!(shape(Some(40.0), 7.0), shape(Some(40.0), 1.0));
    }
}
