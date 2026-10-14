//! Orbit camera: yaw/pitch/distance around a target point.
//!
//! Pure CPU math, no wgpu dependency — testable headless.
//!
//! Orthographic mode ([`Projection::Orthographic`]) keeps the same orbit
//! (yaw/pitch/distance/target) and the same near/far pair as perspective;
//! only the projection matrix changes. Framing parity: the ortho half
//! extents derive from `distance * tan(fov_y / 2)` at the target plane,
//! so toggling projection at a fixed distance keeps the target-plane
//! footprint identical in both modes.

use glam::camera::rh::{proj, view};
use glam::{Mat4, Vec3};

/// Unprojects an NDC point (with explicit depth: z=0 near, z=1 far in
/// wgpu's directx convention) back to world space through the inverse
/// view-projection matrix.
///
/// Shared helper (Wave-4 item 7): viewport picking (`umber-app`'s
/// `pick_uv`) builds its near/far ray endpoints through this, the ground
/// grid's offscreen-test math ports it to predict world positions per
/// pixel, and the grid shader carries its own WGSL copy (`GRID_SHADER`'s
/// `world_from_ndc` — same formula, documented there). The shader copy
/// can't be deduped across the language boundary; the two Rust call
/// sites share this one function instead of each hand-rolling the
/// perspective divide.
pub fn world_from_ndc(inv_view_proj: Mat4, ndc: glam::Vec4) -> Vec3 {
    let world = inv_view_proj * ndc;
    world.truncate() / world.w
}

/// Keeps the camera from flipping over the pole (gimbal lock at the top).
const PITCH_LIMIT: f32 = std::f32::consts::FRAC_PI_2 - 0.01;

/// Which projection the orbit camera renders with.
///
/// V1 boundary: perspective is the default and framing path; orthographic
/// is a view aid (modelling/checking proportions). The post-effects depth
/// of field stays perspective-only (see the viewport's route note), so
/// orthographic frames render on the direct path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Projection {
    /// Today's render: `directx::perspective`.
    #[default]
    Perspective,
    /// Parallel projection: `directx::orthographic`, sized to match the
    /// perspective footprint at the target plane (see
    /// [`OrbitCamera::projection_matrix`]).
    Orthographic,
}

/// An orbit (turntable) camera: it always looks at `target`, positioned at
/// `distance` along the direction given by `yaw`/`pitch`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    /// World-space point the camera always looks at.
    pub target: Vec3,
    /// Rotation around the world Y axis, radians.
    pub yaw: f32,
    /// Rotation up/down from the horizontal plane, radians.
    pub pitch: f32,
    /// Distance from `target` to the eye.
    pub distance: f32,
    /// Lower clamp for `distance` (see [`Self::zoom`]).
    pub min_distance: f32,
    /// Upper clamp for `distance` (see [`Self::zoom`]).
    pub max_distance: f32,
    /// Vertical field of view, radians.
    pub fov_y_radians: f32,
    /// Near clip plane distance.
    pub near: f32,
    /// Far clip plane distance.
    pub far: f32,
    /// Which projection [`Self::projection_matrix`] builds. Preserved
    /// across [`Self::framing`] by the caller (the viewport keeps it on
    /// mesh reload, like yaw/pitch).
    pub projection: Projection,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            yaw: 0.6,
            pitch: 0.4,
            distance: 5.0,
            min_distance: 0.05,
            max_distance: 1000.0,
            fov_y_radians: 45.0_f32.to_radians(),
            near: 0.01,
            far: 1000.0,
            projection: Projection::Perspective,
        }
    }
}

impl OrbitCamera {
    /// Builds a camera framing the given world-space bounding box, keeping
    /// the current yaw/pitch (so re-loading a mesh doesn't spin the view).
    pub fn framing(min: Vec3, max: Vec3, yaw: f32, pitch: f32) -> Self {
        let center = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(0.01);
        let fov = 45.0_f32.to_radians();
        let distance = (radius / (fov * 0.5).tan()) * 1.5;
        Self {
            target: center,
            yaw,
            pitch,
            distance,
            min_distance: radius * 0.02,
            max_distance: distance * 50.0,
            fov_y_radians: fov,
            near: (radius * 0.01).max(0.001),
            far: (distance + radius) * 20.0,
            projection: Projection::Perspective,
        }
    }

    /// World-space eye position.
    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        self.target
            + Vec3::new(
                self.distance * cp * self.yaw.sin(),
                self.distance * self.pitch.sin(),
                self.distance * cp * self.yaw.cos(),
            )
    }

    /// World-to-view transform for the current eye/target/up.
    pub fn view_matrix(&self) -> Mat4 {
        view::look_at_mat4(self.eye(), self.target, Vec3::Y)
    }

    /// `aspect` is width / height; callers must guard against zero-height
    /// viewports before calling this.
    ///
    /// Uses the right-handed, `directx`-convention projection (depth range
    /// 0..1, NDC Y not flipped) — glam 0.34 moved camera math into the
    /// `glam::camera` module with per-API NDC conventions (see
    /// `glam-0.34.1/tests/camera.rs`), and `directx`'s convention is the one
    /// that matches wgpu's clip space.
    ///
    /// Orthographic sizing: `half_h = distance * tan(fov_y / 2)` at the
    /// target plane, `half_w = half_h * aspect` — the ortho footprint at
    /// the target plane equals the perspective frustum's footprint there,
    /// so toggling projection reframes nothing. `near` stays the camera's
    /// own near: glam's `directx::orthographic` takes eye-relative
    /// distances (like the perspective arm — view-space -z from the eye),
    /// not target-relative ones, so the same clip pair keeps the same
    /// depth slab in both modes.
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        match self.projection {
            Projection::Perspective => {
                proj::directx::perspective(self.fov_y_radians, aspect, self.near, self.far)
            }
            Projection::Orthographic => {
                let half_h = self.distance * (self.fov_y_radians * 0.5).tan();
                let half_w = half_h * aspect;
                proj::directx::orthographic(-half_w, half_w, -half_h, half_h, self.near, self.far)
            }
        }
    }

    /// Combined projection * view matrix for the given viewport aspect.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }

    /// Flips between [`Projection::Perspective`] and
    /// [`Projection::Orthographic`], keeping every orbit parameter (the
    /// framing parity in [`Self::projection_matrix`] makes the toggle
    /// reframing-free at the target plane).
    pub fn toggle_projection(&mut self) {
        self.projection = match self.projection {
            Projection::Perspective => Projection::Orthographic,
            Projection::Orthographic => Projection::Perspective,
        };
    }

    /// Drag-orbit: `delta` is in UI pixels (or any consistent unit); scale
    /// is the caller's responsibility via `sensitivity`.
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Pans the target along the camera's current right/up axes, scaled by
    /// distance so panning feels consistent regardless of zoom level.
    pub fn pan(&mut self, delta_x: f32, delta_y: f32) {
        let forward = (self.target - self.eye()).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        let scale = self.distance;
        self.target += right * (delta_x * scale) + up * (delta_y * scale);
    }

    /// Wheel-zoom; `delta` > 0 zooms in. Multiplicative so it feels uniform
    /// near and far from the target.
    pub fn zoom(&mut self, delta: f32) {
        self.distance =
            (self.distance * (1.0 - delta).max(0.01)).clamp(self.min_distance, self.max_distance);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_centers_on_bounds_and_keeps_orientation() {
        let cam = OrbitCamera::framing(
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
            0.6,
            0.4,
        );
        assert_eq!(cam.target, Vec3::ZERO);
        assert_eq!(cam.yaw, 0.6);
        assert_eq!(cam.pitch, 0.4);
        assert!(cam.distance > 0.0);
        assert!(cam.near > 0.0 && cam.near < cam.far);
    }

    #[test]
    fn framing_degenerate_bounds_does_not_panic() {
        let cam = OrbitCamera::framing(Vec3::ZERO, Vec3::ZERO, 0.0, 0.0);
        assert!(cam.distance.is_finite() && cam.distance > 0.0);
    }

    #[test]
    fn pitch_is_clamped_past_the_poles() {
        let mut cam = OrbitCamera::default();
        cam.orbit(0.0, 10.0);
        assert!(cam.pitch <= PITCH_LIMIT);
        cam.orbit(0.0, -20.0);
        assert!(cam.pitch >= -PITCH_LIMIT);
    }

    #[test]
    fn zoom_is_clamped_to_min_max_distance() {
        let mut cam = OrbitCamera {
            min_distance: 1.0,
            max_distance: 10.0,
            distance: 5.0,
            ..Default::default()
        };
        for _ in 0..100 {
            cam.zoom(0.5);
        }
        assert!(cam.distance >= cam.min_distance);
        for _ in 0..100 {
            cam.zoom(-0.9);
        }
        assert!(cam.distance <= cam.max_distance);
    }

    #[test]
    fn view_proj_is_finite_for_sane_aspect() {
        let cam = OrbitCamera::default();
        let m = cam.view_proj(16.0 / 9.0);
        assert!(m.to_cols_array().iter().all(|v| v.is_finite()));
    }

    #[test]
    fn world_from_ndc_roundtrips_through_view_proj() {
        let cam = OrbitCamera::default();
        let view_proj = cam.view_proj(1.0);
        let inv = view_proj.inverse();
        // World origin must survive the round trip at several depths.
        for z in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let clip = view_proj * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
            let ndc = clip.truncate() / clip.w;
            let back = world_from_ndc(inv, glam::Vec4::new(ndc.x, ndc.y, z, 1.0));
            assert!(back.is_finite());
            // At the projected depth the round trip is exact.
            if (z - ndc.z).abs() < 1e-6 {
                assert!(
                    (back - Vec3::ZERO).length() < 1e-4,
                    "round trip drifted: {back:?}"
                );
            }
        }
    }

    /// A head-on ortho camera (yaw = pitch = 0 puts the eye on +z looking
    /// at the target): right is +x, up is +y.
    fn head_on_ortho() -> OrbitCamera {
        OrbitCamera {
            target: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            projection: Projection::Orthographic,
            ..OrbitCamera::default()
        }
    }

    #[test]
    fn default_and_framing_are_perspective_and_toggle_round_trips() {
        assert_eq!(OrbitCamera::default().projection, Projection::Perspective);
        let framed = OrbitCamera::framing(Vec3::new(-1.0, -1.0, -1.0), Vec3::ONE, 0.6, 0.4);
        assert_eq!(framed.projection, Projection::Perspective);
        let mut cam = OrbitCamera::default();
        cam.toggle_projection();
        assert_eq!(cam.projection, Projection::Orthographic);
        cam.toggle_projection();
        assert_eq!(cam.projection, Projection::Perspective);
    }

    #[test]
    fn ortho_frustum_corner_maps_to_ndc_one_one() {
        // Framing parity: (target + right*half_w + up*half_h) is the
        // frustum's top-right edge at the target plane, so it must land
        // on NDC (1, 1) — pinning the half-extent arithmetic. A wrong
        // half width/height (or a swapped aspect) misses by far more than
        // the 1e-5 tolerance.
        let cam = head_on_ortho();
        let aspect = 2.0_f32;
        let half_h = cam.distance * (cam.fov_y_radians * 0.5).tan();
        let half_w = half_h * aspect;
        let corner = cam.target + Vec3::X * half_w + Vec3::Y * half_h;
        let clip = cam.view_proj(aspect) * corner.extend(1.0);
        // Orthographic clip keeps w = 1 (no perspective divide); assert
        // it so a divide-by-w regression can't hide behind the ratio.
        assert!((clip.w - 1.0).abs() < 1e-6, "ortho w must stay 1: {clip:?}");
        let ndc = clip.truncate() / clip.w;
        assert!(
            (ndc.x - 1.0).abs() < 1e-5 && (ndc.y - 1.0).abs() < 1e-5,
            "corner must map to NDC (1, 1), got {ndc:?}"
        );
    }

    #[test]
    fn ortho_view_proj_round_trips_via_world_from_ndc() {
        // Same-shape sibling of the perspective round trip above: at the
        // projected depth the unprojection is exact. w = 1 throughout
        // (see the corner test), so the divide is a no-op — still routed
        // through the shared helper, never hand-rolled.
        let cam = head_on_ortho();
        let view_proj = cam.view_proj(1.0);
        let inv = view_proj.inverse();
        let world = Vec3::new(0.3, -0.2, 1.0);
        let clip = view_proj * world.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        let back = world_from_ndc(inv, ndc.extend(1.0));
        assert!(
            (back - world).length() < 1e-5,
            "ortho round trip drifted: {back:?} vs {world:?}"
        );
    }

    #[test]
    fn ortho_depth_does_not_move_ndc_xy() {
        // Parallel projection: two points on the same sight ray (offset
        // only along view-space -z) collapse to the same NDC xy. A
        // perspective matrix separates them, so this fails loudly there.
        let cam = head_on_ortho();
        let view_proj = cam.view_proj(1.5);
        let project_xy = |p: Vec3| {
            let clip = view_proj * p.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            (ndc.x, ndc.y)
        };
        let (x0, y0) = project_xy(Vec3::new(0.3, -0.2, 0.0));
        let (x1, y1) = project_xy(Vec3::new(0.3, -0.2, 2.0));
        assert!(
            (x0 - x1).abs() < 1e-5 && (y0 - y1).abs() < 1e-5,
            "ortho must collapse depth: ({x0}, {y0}) vs ({x1}, {y1})"
        );
    }
}
