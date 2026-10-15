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
//! - **Painted ring, not an OS bitmap cursor.** photocraft hands a
//!   rasterized outline to the OS so it never lags the pointer. That is
//!   the planned follow-up (platform fallbacks needed: Wayland at
//!   non-1x scale, web, oversize tips). The painted ring trails the
//!   pointer by one frame, which Painter users accept today.
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

#[cfg(test)]
mod tests {
    use super::*;

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
