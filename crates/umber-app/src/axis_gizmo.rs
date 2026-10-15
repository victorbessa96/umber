//! The viewport's axis gizmo (GUI plan G1 `g1-axis-gizmo`,
//! `docs/specs/gui-plan-2026-10-15.md`; sources: Painter A9 / C4 ("Axis
//! gizmo: XYZ at the bottom-right of the 3D view"), Blender C2.1 (click an
//! axis to align the view, click again for the opposite side)).
//!
//! A small orientation widget in the viewport's bottom-right corner. It
//! shows where the world X / Y / Z axes point under the current camera,
//! and clicking an axis tip snaps the view to look down that axis.
//! Clicking the same tip again flips to the opposite side. Pen and tablet
//! users get axis views without keys, and newcomers see where they are.
//!
//! Everything is pure except [`paint`]: [`project_axes`] maps the camera's
//! yaw/pitch to 2D tip offsets with depths, [`hit`] resolves a click to an
//! axis, and [`snap_angles`] gives the yaw/pitch for a view down an axis.
//! The camera convention is `OrbitCamera::eye`: the eye sits at
//! `target + d * (cos(p) sin(y), sin(p), cos(p) cos(y))`, looking at the
//! target with world +Y up.

use egui::{Color32, Pos2, Stroke, Vec2};
use glam::Vec3;

/// Gizmo radius (tip distance from its centre), px.
pub const RADIUS: f32 = 34.0;
/// Margin from the viewport's corner, px.
pub const MARGIN: f32 = 14.0;
/// Click radius around a tip, px.
pub const TIP_HIT_PX: f32 = 9.0;
/// Pitch used for a view down ±Y: the camera clamps pitch just short of
/// the pole (`PITCH_LIMIT`), so a top / bottom view sits at the limit.
const POLE_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.011;

/// A world axis direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl Axis {
    /// All six, in draw-independent order.
    pub const ALL: [Axis; 6] = [
        Axis::PosX,
        Axis::NegX,
        Axis::PosY,
        Axis::NegY,
        Axis::PosZ,
        Axis::NegZ,
    ];

    /// The world unit vector.
    pub fn dir(self) -> Vec3 {
        match self {
            Axis::PosX => Vec3::X,
            Axis::NegX => Vec3::NEG_X,
            Axis::PosY => Vec3::Y,
            Axis::NegY => Vec3::NEG_Y,
            Axis::PosZ => Vec3::Z,
            Axis::NegZ => Vec3::NEG_Z,
        }
    }

    /// The axis pointing the other way.
    pub fn opposite(self) -> Axis {
        match self {
            Axis::PosX => Axis::NegX,
            Axis::NegX => Axis::PosX,
            Axis::PosY => Axis::NegY,
            Axis::NegY => Axis::PosY,
            Axis::PosZ => Axis::NegZ,
            Axis::NegZ => Axis::PosZ,
        }
    }

    /// Positive axes carry a letter and a solid tip; negatives a hollow one.
    pub fn is_positive(self) -> bool {
        matches!(self, Axis::PosX | Axis::PosY | Axis::PosZ)
    }

    /// Label for the positive tips (not translated: X / Y / Z are the
    /// same in every locale Umber ships).
    pub fn label(self) -> &'static str {
        match self {
            Axis::PosX | Axis::NegX => "X",
            Axis::PosY | Axis::NegY => "Y",
            Axis::PosZ | Axis::NegZ => "Z",
        }
    }

    /// The conventional axis colour (X red, Y green, Z blue).
    pub fn color(self) -> Color32 {
        match self {
            Axis::PosX | Axis::NegX => Color32::from_rgb(0xE5, 0x53, 0x4B),
            Axis::PosY | Axis::NegY => Color32::from_rgb(0x6C, 0xC2, 0x4A),
            Axis::PosZ | Axis::NegZ => Color32::from_rgb(0x4A, 0x8C, 0xE5),
        }
    }
}

/// The camera's right / up / back (towards the eye) basis for `yaw`,
/// `pitch` (the `OrbitCamera` convention, world +Y up).
pub fn camera_basis(yaw: f32, pitch: f32) -> (Vec3, Vec3, Vec3) {
    let cp = pitch.cos();
    let back = Vec3::new(cp * yaw.sin(), pitch.sin(), cp * yaw.cos());
    let forward = -back;
    let right = forward.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(forward).normalize_or_zero();
    (right, up, back)
}

/// One projected tip: the screen offset from the gizmo centre (y down) and
/// the depth (positive = towards the viewer, drawn on top).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tip {
    pub axis: Axis,
    pub offset: Vec2,
    pub depth: f32,
}

/// Projects all six axis tips under the camera, sorted back to front (draw
/// order: the nearest tip is drawn last, so it wins).
pub fn project_axes(yaw: f32, pitch: f32, radius: f32) -> Vec<Tip> {
    let (right, up, back) = camera_basis(yaw, pitch);
    let mut tips: Vec<Tip> = Axis::ALL
        .iter()
        .map(|&axis| {
            let d = axis.dir();
            Tip {
                axis,
                // Screen y grows downward.
                offset: Vec2::new(d.dot(right), -d.dot(up)) * radius,
                depth: d.dot(back),
            }
        })
        .collect();
    tips.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    tips
}

/// The axis whose tip is under `pointer_offset` (relative to the gizmo
/// centre), preferring the nearest-to-viewer tip when two overlap.
pub fn hit(tips: &[Tip], pointer_offset: Vec2) -> Option<Axis> {
    tips.iter()
        .rev() // nearest first
        .find(|t| (t.offset - pointer_offset).length() <= TIP_HIT_PX)
        .map(|t| t.axis)
}

/// The yaw / pitch that put the EYE on `axis`'s side of the target, so the
/// view looks down `-axis` (clicking +X shows the mesh from +X). ±Y sit at
/// the camera's pitch limit; their yaw keeps the current one so a top view
/// does not spin.
pub fn snap_angles(axis: Axis, current_yaw: f32) -> (f32, f32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    match axis {
        Axis::PosZ => (0.0, 0.0),
        Axis::NegZ => (PI, 0.0),
        Axis::PosX => (FRAC_PI_2, 0.0),
        Axis::NegX => (-FRAC_PI_2, 0.0),
        Axis::PosY => (current_yaw, POLE_PITCH),
        Axis::NegY => (current_yaw, -POLE_PITCH),
    }
}

/// The axis the camera currently looks down, if it is (nearly) aligned
/// with one: the click-again-for-the-opposite rule needs it.
pub fn aligned_axis(yaw: f32, pitch: f32) -> Option<Axis> {
    let (_, _, back) = camera_basis(yaw, pitch);
    Axis::ALL.into_iter().find(|a| back.dot(a.dir()) > 0.999)
}

/// The axis a click on `clicked` should snap to: `clicked`, or its
/// opposite when the view already looks down `clicked` (Blender C2.1).
pub fn click_target(clicked: Axis, yaw: f32, pitch: f32) -> Axis {
    if aligned_axis(yaw, pitch) == Some(clicked) {
        clicked.opposite()
    } else {
        clicked
    }
}

/// The gizmo's centre for a viewport `rect` (bottom-right corner).
pub fn centre(rect: egui::Rect) -> Pos2 {
    rect.right_bottom() - Vec2::splat(MARGIN + RADIUS)
}

/// Draws the gizmo at `centre`; `hovered` highlights one tip.
pub fn paint(painter: &egui::Painter, centre: Pos2, tips: &[Tip], hovered: Option<Axis>) {
    painter.circle_filled(centre, RADIUS + 10.0, Color32::from_black_alpha(70));
    for tip in tips {
        let at = centre + tip.offset;
        let color = tip.axis.color();
        let strong = hovered == Some(tip.axis);
        if tip.axis.is_positive() {
            painter.line_segment([centre, at], Stroke::new(2.0, color));
            painter.circle_filled(at, if strong { 9.0 } else { 7.5 }, color);
            painter.text(
                at,
                egui::Align2::CENTER_CENTER,
                tip.axis.label(),
                egui::FontId::proportional(10.0),
                Color32::BLACK,
            );
        } else {
            let fill = color.gamma_multiply(if strong { 0.6 } else { 0.25 });
            painter.circle_filled(at, if strong { 7.0 } else { 5.5 }, fill);
            painter.circle_stroke(at, 5.5, Stroke::new(1.0, color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tip(tips: &[Tip], axis: Axis) -> Tip {
        *tips.iter().find(|t| t.axis == axis).expect("tip")
    }

    #[test]
    fn head_on_view_puts_x_right_y_up_z_towards_the_viewer() {
        // yaw 0, pitch 0: the eye is on +Z looking at -Z.
        let tips = project_axes(0.0, 0.0, 10.0);
        let x = tip(&tips, Axis::PosX);
        let y = tip(&tips, Axis::PosY);
        let z = tip(&tips, Axis::PosZ);
        assert!((x.offset - Vec2::new(10.0, 0.0)).length() < 1e-4, "{x:?}");
        assert!((y.offset - Vec2::new(0.0, -10.0)).length() < 1e-4, "{y:?}");
        assert!(z.offset.length() < 1e-4 && z.depth > 0.99, "{z:?}");
        // Sorted back to front: -Z (away) first, +Z (towards) last.
        assert_eq!(tips.first().unwrap().axis, Axis::NegZ);
        assert_eq!(tips.last().unwrap().axis, Axis::PosZ);
    }

    #[test]
    fn hit_finds_the_tip_and_prefers_the_nearer_of_overlapping_tips() {
        let tips = project_axes(0.0, 0.0, 10.0);
        assert_eq!(hit(&tips, Vec2::new(10.0, 1.0)), Some(Axis::PosX));
        assert_eq!(hit(&tips, Vec2::new(-10.0, 0.0)), Some(Axis::NegX));
        // +Z and -Z both project to the centre; +Z is nearer.
        assert_eq!(hit(&tips, Vec2::ZERO), Some(Axis::PosZ));
        assert_eq!(hit(&tips, Vec2::new(30.0, 30.0)), None);
    }

    #[test]
    fn snapping_puts_the_eye_on_the_clicked_axis() {
        for axis in Axis::ALL {
            let (yaw, pitch) = snap_angles(axis, 0.3);
            let (_, _, back) = camera_basis(yaw, pitch);
            assert!(back.dot(axis.dir()) > 0.999, "{axis:?}: back {back:?}");
            assert_eq!(aligned_axis(yaw, pitch), Some(axis), "{axis:?}");
        }
        // A top view keeps the current yaw (no spin).
        assert_eq!(snap_angles(Axis::PosY, 0.7).0, 0.7);
    }

    #[test]
    fn clicking_the_aligned_axis_flips_to_the_opposite_side() {
        let (yaw, pitch) = snap_angles(Axis::PosX, 0.0);
        assert_eq!(click_target(Axis::PosX, yaw, pitch), Axis::NegX);
        assert_eq!(click_target(Axis::PosZ, yaw, pitch), Axis::PosZ);
        // An arbitrary orbit is aligned with nothing.
        assert_eq!(aligned_axis(0.6, 0.3), None);
        assert_eq!(click_target(Axis::PosY, 0.6, 0.3), Axis::PosY);
    }

    #[test]
    fn every_basis_is_orthonormal() {
        for (yaw, pitch) in [(0.0, 0.0), (0.6, 0.3), (2.5, -1.2), (-1.0, 1.5)] {
            let (r, u, b) = camera_basis(yaw, pitch);
            for v in [r, u, b] {
                assert!((v.length() - 1.0).abs() < 1e-4);
            }
            assert!(r.dot(u).abs() < 1e-4 && r.dot(b).abs() < 1e-4 && u.dot(b).abs() < 1e-4);
        }
    }
}
