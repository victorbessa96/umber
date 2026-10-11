//! Fill-layer projections (§2's P0 row): UV, tri-planar, planar, spherical,
//! cylindrical — plus the polygon-fill pixel masks (§2 line 24's P1 row),
//! which gate the projection result per texel (see [`FillParams::mask`]).
//!
//! **Module placement (why here, not in `umber-app`):** every function in
//! this module is a pure function of [`MeshData`] (triangles + UVs +
//! positions + normals) and plain parameters. It needs no GPU device, no
//! tile pool, no egui, and no symmetry tables — exactly the dependency
//! profile of its neighbors [`crate::raycast`] and [`crate::bake_support`].
//! The tile-writing evaluation, the `umber-brush` symmetry wiring, the
//! layer-stack compositing and the egui panel live in
//! `umber-app`'s `fill_layer` module, which calls into here. (`umber-core`
//! already depends on `umber-mesh`, so the layer model referencing
//! [`FillParams`] adds no new crate edge.)
//!
//! **Live, not baked:** a fill layer re-evaluates from its [`FillSource`] on
//! every source/projection change (the app side tracks this with a dirty
//! flag). Nothing here caches pixels; [`evaluate_fill_tile`] is a pure
//! function of `(mesh, params, tile, size)`.
//!
//! # Conventions
//!
//! * Texel↔UV uses the bake convention (`position.rs`): texel `(x, y)`
//!   (row-major, `y = 0` at the TOP) maps to
//!   `u = tile_u + (x + 0.5) / w`, `v = tile_v + 1 - (y + 0.5) / h`, where
//!   `(tile_u, tile_v)` are the UDIM integer offsets of `tile`
//!   (`tile - 1001 = tile_u + 10 * tile_v`, matching
//!   `umber-app`'s `local_texel` inverse).
//! * Fill-source coordinates tile: every sampler wraps with a Euclidean
//!   `fract` ([`wrap01`]), so projections may return any range (world units,
//!   radians, heights) and the pattern repeats instead of clamping.
//! * No panics: every mesh-index and image-index access goes through
//!   [`slice::get`]; malformed triangles, missing attributes and
//!   size-mismatched graph images degrade to "uncovered" / transparent,
//!   never an index panic. Non-finite projection inputs degrade to fixed
//!   fallback coordinates (documented per function).
//! * Tri-planar is a three-sample blend, not a single 2D coordinate, so it
//!   has no [`projection_sample_2d`] arm: use [`triplanar_weights`] +
//!   [`triplanar_planar_uvs`] + [`sample_triplanar`] instead.

use std::mem::size_of;

use serde::{Deserialize, Serialize};

use crate::polygon_fill::FillMask;
use crate::MeshData;

/// A coordinate axis, for the [`FillProjection::Planar`] plane and the
/// [`FillProjection::Cylindrical`] revolution axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProjAxis {
    X,
    /// The up axis (Substance-style default for cylindrical wraps).
    #[default]
    Y,
    Z,
}

impl ProjAxis {
    /// The two non-axis position components, in order: the planar sample
    /// coordinates (and the cylindrical angle pair).
    fn plane(self, pos: [f32; 3]) -> [f32; 2] {
        match self {
            ProjAxis::X => [pos[1], pos[2]],
            ProjAxis::Y => [pos[0], pos[2]],
            ProjAxis::Z => [pos[0], pos[1]],
        }
    }

    /// The axis coordinate (the cylindrical height).
    fn height(self, pos: [f32; 3]) -> f32 {
        match self {
            ProjAxis::X => pos[0],
            ProjAxis::Y => pos[1],
            ProjAxis::Z => pos[2],
        }
    }
}

/// The five fill projections (§2, line 20). The fill layer ships WITH these —
/// there is no projection-less fill (a [`FillProjection::Uv`] identity is
/// the trivial base case, not an absence).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum FillProjection {
    /// The identity: the fill source maps straight onto the UV layout.
    #[default]
    Uv,
    /// The position sampled in each axis-aligned plane, blended by the
    /// dominant-axis weights ([`triplanar_weights`]).
    Triplanar {
        /// Blend sharpness (the standard power-law: higher concentrates the
        /// blend on the dominant plane; `1.0` is linear).
        sharpness: f32,
        /// World-units-per-pattern-tile scale applied to each plane's UV.
        scale: f32,
    },
    /// Projection along one axis: the fill coordinates are the position on
    /// that axis's plane.
    Planar { axis: ProjAxis },
    /// The position mapped to spherical `(theta, phi)`: the fill wraps
    /// around the sphere. `theta = atan2(z, x)` so `+X` maps to `u = 0`.
    Spherical,
    /// The position mapped to `(angle, height)` around one axis: the angle
    /// of the non-axis pair plus the axis coordinate.
    Cylindrical { axis: ProjAxis },
}

/// The fill layer's 2D pattern source (v1).
///
/// `GraphImage` is the "trivially reachable" node-graph wiring, by design a
/// snapshot rather than a live graph reference: the caller evaluates the
/// node graph through whatever engine exists and hands the fill the
/// resulting RGBA image, which the fill samples bilinearly in fill-UV
/// space. This keeps the fill decoupled from `umber-graph`'s API churn;
/// promoting it to a live graph handle (with its own dirty propagation) is
/// the named follow-up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FillSource {
    /// A flat color (linear 0..=1 RGBA).
    Solid { color: [f32; 4] },
    /// A two-stop linear gradient across fill-UV space: `t` is the wrapped
    /// projection of the sample point onto the `angle` direction.
    Gradient {
        a: [f32; 4],
        b: [f32; 4],
        /// Gradient direction, radians (0 = +u).
        angle: f32,
    },
    /// Deterministic CPU value-noise fBm (no new dependency): grayscale
    /// with opaque alpha. Same `(seed, st)` always samples identically.
    Noise {
        seed: u32,
        /// Pattern frequency in fill-UV units.
        scale: f32,
        /// fBm octaves, clamped to `1..=8` at sample time.
        octaves: u32,
    },
    /// A caller-evaluated RGBA image snapshot (see the enum docs).
    GraphImage {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}

impl Default for FillSource {
    fn default() -> Self {
        FillSource::Solid {
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// A fill layer's evaluatable parameters: what to paint + how to project it
/// + where it may paint. Stored on `umber-core`'s `Layer`
/// (`fill: Option<FillParams>`); `None` means "not a fill layer" and keeps
/// [`crate::LayerKind::Fill`] a plain tag so the journal/project matches
/// don't churn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FillParams {
    pub source: FillSource,
    pub projection: FillProjection,
    /// The polygon-fill pixel mask (the P1 row: triangle / polygon lasso /
    /// mesh island / UV chunk), or `None` for the unmasked P0 behavior.
    /// `#[serde(default)]` so projects saved before the polygon-fill row
    /// landed (no such key) still load — as unmasked fills.
    #[serde(default)]
    pub mask: Option<FillMask>,
}

impl FillParams {
    /// Approximate heap held by these params (the graph snapshot's bytes
    /// plus the mask's loop/ID vecs; everything else is inline). Feeds
    /// `Layer::memory_bytes`.
    pub fn memory_bytes(&self) -> usize {
        size_of::<FillParams>()
            + match &self.source {
                FillSource::GraphImage { rgba, .. } => rgba.len(),
                _ => 0,
            }
            + self.mask.as_ref().map(FillMask::memory_bytes).unwrap_or(0)
    }
}

/// Euclidean `fract`: wraps any finite float into `[0, 1)`. Non-finite
/// input maps to `0.0` (a fixed fallback, never NaN propagation).
pub fn wrap01(v: f32) -> f32 {
    if !v.is_finite() {
        return 0.0;
    }
    v - v.floor()
}

/// The single-plane projections' 2D fill-source coordinates for a surface
/// point (`pos` world position, `normal` unit-ish face normal, `uv` mesh UV).
///
/// * [`FillProjection::Uv`] → `uv` (the identity base case).
/// * [`FillProjection::Planar`] → the position on the axis plane:
///   X→`(y, z)`, Y→`(x, z)`, Z→`(x, y)`.
/// * [`FillProjection::Spherical`] → `(theta / TAU mod 1, phi / PI)` with
///   `theta = atan2(z, x)` (`+X` → `u = 0`) and `phi = acos(y / r)`
///   (`+Y` → `v = 0`, the top pole). The origin has no direction and maps
///   to `(0, 0.5)`.
/// * [`FillProjection::Cylindrical`] → `(angle / TAU mod 1, height)` with
///   the angle over the axis's non-axis pair and `height` the axis
///   coordinate.
///
/// [`FillProjection::Triplanar`] has no single coordinate (it blends three
/// samples); see [`triplanar_weights`] / [`triplanar_planar_uvs`].
pub fn projection_sample_2d(pos: [f32; 3], uv: [f32; 2], proj: FillProjection) -> [f32; 2] {
    match proj {
        FillProjection::Uv => uv,
        FillProjection::Planar { axis } => axis.plane(pos),
        FillProjection::Spherical => spherical_coords(pos),
        FillProjection::Cylindrical { axis } => cylindrical_coords(pos, axis),
        FillProjection::Triplanar { .. } => {
            // No single coordinate exists; returning one plane would
            // silently drop two thirds of the blend. Callers must use the
            // tri-planar path. (Unreachable in practice: the evaluator
            // matches on the projection first.)
            axis_plane_fallback(pos)
        }
    }
}

/// Fallback for [`projection_sample_2d`]'s tri-planar arm (see its docs):
/// the Z-plane projection. Never a primary path.
fn axis_plane_fallback(pos: [f32; 3]) -> [f32; 2] {
    [pos[0], pos[1]]
}

/// Spherical `(u, v)` for `pos` (see [`projection_sample_2d`]).
pub fn spherical_coords(pos: [f32; 3]) -> [f32; 2] {
    let r = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
    if !r.is_finite() || r <= 0.0 {
        return [0.0, 0.5];
    }
    let theta = pos[2].atan2(pos[0]);
    let phi = (pos[1] / r).clamp(-1.0, 1.0).acos();
    [
        wrap01(theta / std::f32::consts::TAU),
        (phi / std::f32::consts::PI).clamp(0.0, 1.0),
    ]
}

/// Cylindrical `(u, v)` for `pos` around `axis` (see
/// [`projection_sample_2d`]).
pub fn cylindrical_coords(pos: [f32; 3], axis: ProjAxis) -> [f32; 2] {
    let [a, b] = axis.plane(pos);
    let angle = b.atan2(a);
    [wrap01(angle / std::f32::consts::TAU), axis.height(pos)]
}

/// The standard smooth tri-planar weights: `|n_i|^k / Σ|n_j|^k` over the
/// absolute normal components, with non-finite components treated as 0 and
/// a zero/degenerate normal blending all three planes equally (no NaN, no
/// preferred plane).
pub fn triplanar_weights(normal: [f32; 3], sharpness: f32) -> [f32; 3] {
    let k = if sharpness.is_finite() {
        sharpness.max(0.0)
    } else {
        1.0
    };
    let ax = [
        normal_component_abs(normal[0]),
        normal_component_abs(normal[1]),
        normal_component_abs(normal[2]),
    ];
    let w = [ax[0].powf(k), ax[1].powf(k), ax[2].powf(k)];
    let sum = w[0] + w[1] + w[2];
    if sum > 0.0 {
        [w[0] / sum, w[1] / sum, w[2] / sum]
    } else {
        [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]
    }
}

fn normal_component_abs(c: f32) -> f32 {
    if c.is_finite() {
        c.abs()
    } else {
        0.0
    }
}

/// The three axis-plane sample coordinates for `pos`, in weights order
/// `[X-plane (y, z), Y-plane (x, z), Z-plane (x, y)]`, scaled by `scale`
/// (non-finite/non-positive scales fall back to `1.0`).
pub fn triplanar_planar_uvs(pos: [f32; 3], scale: f32) -> [[f32; 2]; 3] {
    let s = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    [
        [pos[1] * s, pos[2] * s],
        [pos[0] * s, pos[2] * s],
        [pos[0] * s, pos[1] * s],
    ]
}

/// Samples `source` at fill-UV `st` (any range — every arm wraps via
/// [`wrap01`]), returning linear 0..=1 RGBA.
pub fn sample_source(source: &FillSource, st: [f32; 2]) -> [f32; 4] {
    match source {
        FillSource::Solid { color } => *color,
        FillSource::Gradient { a, b, angle } => {
            let dir = if angle.is_finite() {
                [angle.cos(), angle.sin()]
            } else {
                [1.0, 0.0]
            };
            let t = wrap01(st[0] * dir[0] + st[1] * dir[1]);
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
                a[3] + (b[3] - a[3]) * t,
            ]
        }
        FillSource::Noise {
            seed,
            scale,
            octaves,
        } => {
            let s = if scale.is_finite() { *scale } else { 1.0 };
            let oct = (*octaves).clamp(1, 8);
            let v = fbm(*seed, [st[0] * s, st[1] * s], oct);
            [v, v, v, 1.0]
        }
        FillSource::GraphImage {
            width,
            height,
            rgba,
        } => sample_image(*width, *height, rgba, st),
    }
}

/// The tri-planar sample: `source` evaluated in each axis plane and blended
/// by [`triplanar_weights`] — the smooth 3-way blend.
pub fn sample_triplanar(
    source: &FillSource,
    pos: [f32; 3],
    normal: [f32; 3],
    sharpness: f32,
    scale: f32,
) -> [f32; 4] {
    let w = triplanar_weights(normal, sharpness);
    let uvs = triplanar_planar_uvs(pos, scale);
    let (s0, s1, s2) = (
        sample_source(source, uvs[0]),
        sample_source(source, uvs[1]),
        sample_source(source, uvs[2]),
    );
    [
        s0[0] * w[0] + s1[0] * w[1] + s2[0] * w[2],
        s0[1] * w[0] + s1[1] * w[1] + s2[1] * w[2],
        s0[2] * w[0] + s1[2] * w[1] + s2[2] * w[2],
        s0[3] * w[0] + s1[3] * w[1] + s2[3] * w[2],
    ]
}

/// Bilinear sample of an RGBA8 image in fill-UV space (wrapped). Empty
/// images, zero extents, and length-mismatched buffers sample transparent
/// black — never a panic.
fn sample_image(width: u32, height: u32, rgba: &[u8], st: [f32; 2]) -> [f32; 4] {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() != w * h * 4 {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let fx = wrap01(st[0]) * width as f32 - 0.5;
    let fy = wrap01(st[1]) * height as f32 - 0.5;
    let x0 = fx.floor() as i64;
    let y0 = fy.floor() as i64;
    let (tx, ty) = (fx - fx.floor(), fy - fy.floor());
    let px = |x: i64, y: i64| -> [f32; 4] {
        let (x, y) = (
            x.rem_euclid(width as i64) as usize,
            y.rem_euclid(height as i64) as usize,
        );
        let i = (y * w + x) * 4;
        match rgba.get(i..i + 4) {
            Some(px) => [
                px[0] as f32 / 255.0,
                px[1] as f32 / 255.0,
                px[2] as f32 / 255.0,
                px[3] as f32 / 255.0,
            ],
            None => [0.0, 0.0, 0.0, 0.0],
        }
    };
    let (a, b, c, d) = (
        px(x0, y0),
        px(x0 + 1, y0),
        px(x0, y0 + 1),
        px(x0 + 1, y0 + 1),
    );
    let lerp = |p: f32, q: f32| p + (q - p) * tx;
    let top = [
        lerp(a[0], b[0]),
        lerp(a[1], b[1]),
        lerp(a[2], b[2]),
        lerp(a[3], b[3]),
    ];
    let bot = [
        lerp(c[0], d[0]),
        lerp(c[1], d[1]),
        lerp(c[2], d[2]),
        lerp(c[3], d[3]),
    ];
    [
        top[0] + (bot[0] - top[0]) * ty,
        top[1] + (bot[1] - top[1]) * ty,
        top[2] + (bot[2] - top[2]) * ty,
        top[3] + (bot[3] - top[3]) * ty,
    ]
}

/// Deterministic 2D value-noise fBm in `[0, 1]`: `octaves` octaves of
/// smoothstep-interpolated integer-lattice hashes, normalized by the
/// amplitude sum. Same `(seed, p)` always yields the same value on any
/// platform (integer hash — no float-transcendental lattice dependence
/// beyond `floor`).
fn fbm(seed: u32, p: [f32; 2], octaves: u32) -> f32 {
    let (mut amp, mut freq, mut sum, mut norm) = (0.5f32, 1.0f32, 0.0f32, 0.0f32);
    for _ in 0..octaves {
        sum += amp * value_noise(seed, [p[0] * freq, p[1] * freq]);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    if norm > 0.0 {
        (sum / norm).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn value_noise(seed: u32, p: [f32; 2]) -> f32 {
    if !p[0].is_finite() || !p[1].is_finite() {
        return 0.5;
    }
    let (xi, yi) = (p[0].floor() as i32, p[1].floor() as i32);
    let (tx, ty) = (p[0] - p[0].floor(), p[1] - p[1].floor());
    let sx = tx * tx * (3.0 - 2.0 * tx);
    let sy = ty * ty * (3.0 - 2.0 * ty);
    let (a, b, c, d) = (
        hash2(seed, xi, yi),
        hash2(seed, xi + 1, yi),
        hash2(seed, xi, yi + 1),
        hash2(seed, xi + 1, yi + 1),
    );
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

/// Integer lattice hash → `[0, 1)`: wrapping arithmetic only, so identical
/// on every platform (release and debug — no overflow-panic path: all ops
/// are `wrapping_*` / `^` / `>>`).
fn hash2(seed: u32, x: i32, y: i32) -> f32 {
    let mut h = seed
        .wrapping_add((x as u32).wrapping_mul(0x8da6_b343))
        .wrapping_add((y as u32).wrapping_mul(0xd816_3841));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    (h >> 8) as f32 / 16_777_216.0
}

/// Per-texel surface geometry under a tile's UV footprint: what the fill's
/// projection evaluates at that texel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TexelGeom {
    /// World-space position (barycentrically interpolated).
    pub pos: [f32; 3],
    /// The covering triangle's face normal (recomputed from positions, not
    /// read from `mesh.normals` — the OBJ loader can leave those short;
    /// same choice as the bake transfer builders).
    pub normal: [f32; 3],
    /// The texel center's global UV (tile offset included).
    pub uv: [f32; 2],
    /// The covering triangle's index (`mesh.indices / 3` position): what
    /// the polygon-fill topology masks test membership against. Tracks
    /// the same last-in-index-order winner as the geometry itself.
    pub triangle: usize,
}

/// Rasterizes `mesh`'s UV layout into `tile`'s `width`x`height` texel grid:
/// one [`TexelGeom`] per texel covered by a triangle's UV footprint, `None`
/// elsewhere.
///
/// Walks `mesh.indices.chunks_exact(3)` exactly like the bake passes'
/// triangle builders; malformed triangles (out-of-range position/UV
/// indices) and UV-degenerate triangles are skipped, never panics. When
/// two triangles cover one texel the LAST in index order wins (overlapping
/// UV islands are an authoring error; a deterministic rule beats a random
/// one). Zero-sized targets yield an empty vec.
pub fn rasterize_uv_geometry(
    mesh: &MeshData,
    tile: u16,
    width: u32,
    height: u32,
) -> Vec<Option<TexelGeom>> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 {
        return Vec::new();
    }
    // UDIM integer offsets (the `local_texel` inverse: u = tu + x / tpv,
    // v = tv + 1 - y / tpv).
    let index = tile.saturating_sub(crate::FIRST_TILE);
    let (tu, tv) = (f32::from(index % 10), f32::from(index / 10));
    let mut out: Vec<Option<TexelGeom>> = vec![None; w * h];
    for (tri_idx, tri) in mesh.indices.chunks_exact(3).enumerate() {
        let (Some(&i0), Some(&i1), Some(&i2)) = (tri.first(), tri.get(1), tri.get(2)) else {
            continue;
        };
        let (Some(&p0), Some(&p1), Some(&p2)) = (
            mesh.positions.get(i0 as usize),
            mesh.positions.get(i1 as usize),
            mesh.positions.get(i2 as usize),
        ) else {
            continue;
        };
        let (Some(&uv0), Some(&uv1), Some(&uv2)) = (
            mesh.uvs.get(i0 as usize),
            mesh.uvs.get(i1 as usize),
            mesh.uvs.get(i2 as usize),
        ) else {
            continue;
        };
        if !uv_finite([uv0, uv1, uv2]) || !pos_finite([p0, p1, p2]) {
            continue;
        }
        // Signed UV-space area; degenerate footprints cover nothing.
        let area = (uv1[0] - uv0[0]) * (uv2[1] - uv0[1]) - (uv2[0] - uv0[0]) * (uv1[1] - uv0[1]);
        if !area.is_finite() || area.abs() < 1e-12 {
            continue;
        }
        // Face normal from positions (see `TexelGeom::normal`).
        let ax = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let bx = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let n = [
            ax[1] * bx[2] - ax[2] * bx[1],
            ax[2] * bx[0] - ax[0] * bx[2],
            ax[0] * bx[1] - ax[1] * bx[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let normal = if len > 0.0 {
            [n[0] / len, n[1] / len, n[2] / len]
        } else {
            [0.0, 0.0, 0.0]
        };
        // UV bbox → tile-local texel range (expanded by one texel so
        // texel-center coverage at footprint edges is exact, then clamped).
        let (min_u, max_u) = (
            uv0[0].min(uv1[0]).min(uv2[0]),
            uv0[0].max(uv1[0]).max(uv2[0]),
        );
        let (min_v, max_v) = (
            uv0[1].min(uv1[1]).min(uv2[1]),
            uv0[1].max(uv1[1]).max(uv2[1]),
        );
        let x0 = (((min_u - tu) * width as f32 - 1.0).floor() as i64).max(0) as usize;
        let x1 = ((((max_u - tu) * width as f32 + 1.0).ceil() as i64).min(width as i64 - 1)).max(0)
            as usize;
        let y0 = ((((tv + 1.0 - max_v) * height as f32 - 1.0).floor() as i64).max(0)) as usize;
        let y1 = (((((tv + 1.0 - min_v) * height as f32 + 1.0).ceil() as i64)
            .min(height as i64 - 1))
        .max(0)) as usize;
        if x0 > x1 || y0 > y1 {
            continue;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                let uv = [
                    tu + (x as f32 + 0.5) / width as f32,
                    tv + 1.0 - (y as f32 + 0.5) / height as f32,
                ];
                // Barycentrics via edge functions over the signed area
                // (winding-agnostic: the inside test follows the area sign).
                let e0 =
                    (uv[0] - uv1[0]) * (uv2[1] - uv1[1]) - (uv[1] - uv1[1]) * (uv2[0] - uv1[0]);
                let e1 =
                    (uv[0] - uv2[0]) * (uv0[1] - uv2[1]) - (uv[1] - uv2[1]) * (uv0[0] - uv2[0]);
                let e2 =
                    (uv[0] - uv0[0]) * (uv1[1] - uv0[1]) - (uv[1] - uv0[1]) * (uv1[0] - uv0[0]);
                let (b0, b1, b2) = (e0 / area, e1 / area, e2 / area);
                let inside = if area > 0.0 {
                    b0 >= -1e-6 && b1 >= -1e-6 && b2 >= -1e-6
                } else {
                    b0 <= 1e-6 && b1 <= 1e-6 && b2 <= 1e-6
                };
                if !inside {
                    continue;
                }
                out[y * w + x] = Some(TexelGeom {
                    pos: [
                        p0[0] * b0 + p1[0] * b1 + p2[0] * b2,
                        p0[1] * b0 + p1[1] * b1 + p2[1] * b2,
                        p0[2] * b0 + p1[2] * b1 + p2[2] * b2,
                    ],
                    normal,
                    uv,
                    triangle: tri_idx,
                });
            }
        }
    }
    out
}

fn uv_finite(uvs: [[f32; 2]; 3]) -> bool {
    uvs.iter().all(|uv| uv[0].is_finite() && uv[1].is_finite())
}

fn pos_finite(ps: [[f32; 3]; 3]) -> bool {
    ps.iter()
        .all(|p| p[0].is_finite() && p[1].is_finite() && p[2].is_finite())
}

/// A fill layer's evaluated tile: RGBA8 bytes plus the 0..=1 coverage mask,
/// both row-major top-down (`y = 0` is the top row, matching the paint
/// targets). Uncovered texels are transparent black with coverage `0.0`.
///
/// This is the RAW projection result — before symmetry expansion and before
/// layer-stack compositing, both of which `umber-app`'s `fill_layer` module
/// applies on top.
pub struct FillTile {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub coverage: Vec<f32>,
}

/// Evaluates `params` over `tile`'s `width`x`height` grid: for each texel
/// the projection maps the texel's surface point into the fill source's 2D
/// space, samples it, and writes the byte — unless `params.mask` excludes
/// the texel, which stays transparent with coverage `0.0` (the polygon
/// fill composes with every projection through this one gate, before
/// symmetry and layer compositing). Pure — no caching here (the app-side
/// dirty flag decides WHEN to call this, not what it returns).
pub fn evaluate_fill_tile(
    mesh: &MeshData,
    params: &FillParams,
    tile: u16,
    width: u32,
    height: u32,
) -> FillTile {
    let (w, h) = (width as usize, height as usize);
    let mut rgba = vec![0u8; w * h * 4];
    let mut coverage = vec![0.0f32; w * h];
    if w == 0 || h == 0 {
        return FillTile {
            width,
            height,
            rgba,
            coverage,
        };
    }
    let geom = rasterize_uv_geometry(mesh, tile, width, height);
    debug_assert_eq!(geom.len(), w * h);
    for (i, g) in geom.iter().enumerate() {
        let Some(g) = g else {
            continue;
        };
        if let Some(mask) = params.mask.as_ref() {
            if !mask.covers(g.uv, g.triangle) {
                continue;
            }
        }
        let sample = match params.projection {
            FillProjection::Triplanar { sharpness, scale } => {
                sample_triplanar(&params.source, g.pos, g.normal, sharpness, scale)
            }
            other => {
                let st = projection_sample_2d(g.pos, g.uv, other);
                sample_source(&params.source, st)
            }
        };
        coverage[i] = 1.0;
        rgba[i * 4] = (sample[0].clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8;
        rgba[i * 4 + 1] = (sample[1].clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8;
        rgba[i * 4 + 2] = (sample[2].clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8;
        rgba[i * 4 + 3] = (sample[3].clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8;
    }
    FillTile {
        width,
        height,
        rgba,
        coverage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < EPS
    }

    /// Flat quad at z=0 with full [0,1] UVs (mirrors raycast's fixture).
    fn full_uv_quad() -> MeshData {
        MeshData {
            positions: vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
        }
    }

    /// Quad whose UVs fill [0.5, 1]²: only the top-right quadrant of the
    /// tile is occupied (mirrors position.rs `half_uv_quad`).
    fn half_uv_quad() -> MeshData {
        let mut mesh = full_uv_quad();
        mesh.uvs = vec![[0.5, 0.5], [1.0, 0.5], [1.0, 1.0], [0.5, 1.0]];
        mesh
    }

    /// Axis-aligned cube, faces at ±1, outward windings (each verified by
    /// hand: the face's cross product points along its outward normal).
    ///
    /// 24 vertices (4 per face): each face owns a disjoint cell of a 3x2
    /// UV grid, so every face rasterizes without overlap (shared-vertex
    /// UVs would collapse side faces to degenerate footprints, and
    /// full-quad-per-face UVs would stack all six footprints on top of
    /// each other).
    fn unit_cube() -> MeshData {
        // (face normal axis, sign, quad corners a/b/c/d for tris
        // (a,b,c),(a,c,d)).
        type QuadTable = [([f32; 3], [f32; 3], [f32; 3], [f32; 3]); 6];
        let faces: QuadTable = [
            // -X
            (
                [-1.0, -1.0, -1.0],
                [-1.0, -1.0, 1.0],
                [-1.0, 1.0, 1.0],
                [-1.0, 1.0, -1.0],
            ),
            // +X
            (
                [1.0, -1.0, -1.0],
                [1.0, 1.0, -1.0],
                [1.0, 1.0, 1.0],
                [1.0, -1.0, 1.0],
            ),
            // -Y
            (
                [-1.0, -1.0, -1.0],
                [1.0, -1.0, -1.0],
                [1.0, -1.0, 1.0],
                [-1.0, -1.0, 1.0],
            ),
            // +Y
            (
                [-1.0, 1.0, -1.0],
                [-1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 1.0, -1.0],
            ),
            // -Z
            (
                [-1.0, -1.0, -1.0],
                [-1.0, 1.0, -1.0],
                [1.0, 1.0, -1.0],
                [1.0, -1.0, -1.0],
            ),
            // +Z
            (
                [-1.0, -1.0, 1.0],
                [1.0, -1.0, 1.0],
                [1.0, 1.0, 1.0],
                [-1.0, 1.0, 1.0],
            ),
        ];
        let mut positions = Vec::with_capacity(24);
        let mut uvs = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);
        for (f, (a, b, c, d)) in faces.iter().enumerate() {
            let base = positions.len() as u32;
            positions.extend_from_slice(&[*a, *b, *c, *d]);
            let (col, row) = (f % 3, f / 3);
            let (u0, v0) = (col as f32 / 3.0, row as f32 / 2.0);
            let (u1, v1) = ((col + 1) as f32 / 3.0, (row + 1) as f32 / 2.0);
            uvs.extend_from_slice(&[[u0, v0], [u1, v0], [u1, v1], [u0, v1]]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        MeshData {
            positions,
            normals: vec![],
            uvs,
            indices,
            material_names: vec![],
        }
    }

    /// A `w`x`h` graph image whose pixel encodes its own normalized coords
    /// as raised cosines: `R = (cos(2πu)/2 + 1/2) * 255`,
    /// `G = (cos(2πv)/2 + 1/2) * 255`, `B = 0`, `A = 255`. The cosine is
    /// wrap-continuous (`u = 0` and `u = 1` encode identically), so bilinear
    /// filtering across the tile seam stays exact — a linear ramp would
    /// tear there and be unassertable at integer coordinates (exactly where
    /// cube corners land). Lets projection tests read back WHERE a texel
    /// sampled from.
    fn coord_image(w: u32, h: u32) -> FillSource {
        let enc = |t: f32| ((t * std::f32::consts::TAU).cos() * 0.5 + 0.5) * 255.0;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.push((enc((x as f32 + 0.5) / w as f32) + 0.5).floor() as u8);
                rgba.push((enc((y as f32 + 0.5) / h as f32) + 0.5).floor() as u8);
                rgba.push(0);
                rgba.push(255);
            }
        }
        FillSource::GraphImage {
            width: w,
            height: h,
            rgba,
        }
    }

    /// The test pattern's ground truth at fill-UV `st` (the same cosine
    /// the image encodes; the wrap makes `fract` exact here).
    fn coord_expected(st: [f32; 2]) -> [f32; 2] {
        let enc = |t: f32| ((wrap01(t) * std::f32::consts::TAU).cos() * 0.5 + 0.5) * 255.0;
        [enc(st[0]), enc(st[1])]
    }

    fn solid_params(color: [f32; 4]) -> FillParams {
        FillParams {
            source: FillSource::Solid { color },
            projection: FillProjection::Uv,
            mask: None,
        }
    }

    /// Test (a): UV identity — a solid fill covers the tile's occupied UV
    /// region exactly, and nothing else.
    #[test]
    fn uv_identity_covers_only_the_occupied_region() {
        let red = [1.0, 0.0, 0.0, 1.0];
        let tile = evaluate_fill_tile(&half_uv_quad(), &solid_params(red), 1001, 16, 16);
        assert_eq!(tile.rgba.len(), 16 * 16 * 4);
        assert_eq!(tile.coverage.len(), 16 * 16);
        // UVs in [0.5, 1]² = right half (x >= 8) + top half (y < 8).
        for y in 0..16 {
            for x in 0..16 {
                let i = y * 16 + x;
                let px = &tile.rgba[i * 4..i * 4 + 4];
                if x >= 8 && y < 8 {
                    assert_eq!(px, &[255, 0, 0, 255], "occupied texel ({x},{y})");
                    assert_eq!(tile.coverage[i], 1.0);
                } else {
                    assert_eq!(px, &[0, 0, 0, 0], "unoccupied texel ({x},{y})");
                    assert_eq!(tile.coverage[i], 0.0);
                }
            }
        }
    }

    /// The full-UV quad is covered everywhere (the identity's other half).
    #[test]
    fn uv_identity_full_quad_covers_every_texel() {
        let tile = evaluate_fill_tile(
            &full_uv_quad(),
            &solid_params([0.0, 1.0, 0.0, 1.0]),
            1001,
            8,
            8,
        );
        assert!(tile.coverage.iter().all(|&c| c == 1.0));
        assert!(tile.rgba.chunks_exact(4).all(|px| px == [0, 255, 0, 255]));
        // And the rasterizer's interpolated positions match the analytic
        // UV→world map pos = (2u - 1, 2v - 1, 0).
        let geom = rasterize_uv_geometry(&full_uv_quad(), 1001, 8, 8);
        for (i, g) in geom.iter().enumerate() {
            let g = g.expect("full coverage");
            let x = i % 8;
            let y = i / 8;
            let (u, v) = ((x as f32 + 0.5) / 8.0, 1.0 - (y as f32 + 0.5) / 8.0);
            assert!((g.pos[0] - (2.0 * u - 1.0)).abs() < 1e-4, "{g:?}");
            assert!((g.pos[1] - (2.0 * v - 1.0)).abs() < 1e-4, "{g:?}");
            assert!(g.pos[2].abs() < 1e-6);
            assert!((g.normal[2] - 1.0).abs() < 1e-6, "face normal +z: {g:?}");
        }
    }

    /// Axis-aligned normals concentrate the whole blend on their plane;
    /// the diagonal splits it evenly; degenerate input degrades to thirds.
    #[test]
    fn triplanar_weights_pick_the_dominant_axis() {
        for (n, want) in [
            ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            ([-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [0.0, 0.0, 1.0]),
        ] {
            let w = triplanar_weights(n, 2.0);
            assert!(
                near(w[0], want[0]) && near(w[1], want[1]) && near(w[2], want[2]),
                "{n:?} -> {w:?}"
            );
        }
        let d = 1.0 / 3.0f32.sqrt();
        let w = triplanar_weights([d, d, d], 2.0);
        assert!(
            near(w[0], 1.0 / 3.0) && near(w[1], 1.0 / 3.0) && near(w[2], 1.0 / 3.0),
            "diagonal: {w:?}"
        );
        // Sharpness concentrates: mostly-X leans harder with higher k.
        let soft = triplanar_weights([0.9, 0.1, 0.0], 1.0);
        let sharp = triplanar_weights([0.9, 0.1, 0.0], 8.0);
        assert!(sharp[0] > soft[0], "{soft:?} vs {sharp:?}");
        // Degenerate normals blend equally instead of NaN.
        for n in [
            [0.0, 0.0, 0.0],
            [f32::NAN, 0.0, 1.0],
            [f32::INFINITY, 0.0, 0.0],
        ] {
            let w = triplanar_weights(n, 2.0);
            assert!(
                near(w[0], 1.0 / 3.0) && near(w[1], 1.0 / 3.0) && near(w[2], 1.0 / 3.0),
                "{n:?} -> {w:?}"
            );
        }
    }

    /// Test (b): tri-planar on a cube — every covered texel samples its
    /// dominant plane (axis-aligned faces concentrate the blend exactly),
    /// all six faces appear, and the corner texels assert against their own
    /// rasterized geometry (projection + sampler path, not the rasterizer).
    #[test]
    fn triplanar_on_cube_samples_the_dominant_plane() {
        let params = FillParams {
            source: coord_image(64, 64),
            projection: FillProjection::Triplanar {
                sharpness: 2.0,
                scale: 1.0,
            },
            mask: None,
        };
        let size = 32u32;
        let tile = evaluate_fill_tile(&unit_cube(), &params, 1001, size, size);
        let geom = rasterize_uv_geometry(&unit_cube(), 1001, size, size);
        let mut faces_seen = [false; 6];
        let mut covered = 0;
        for y in 0..size {
            for x in 0..size {
                let i = (y * size + x) as usize;
                let Some(g) = geom[i] else {
                    assert_eq!(tile.coverage[i], 0.0);
                    assert_eq!(&tile.rgba[i * 4..i * 4 + 4], &[0, 0, 0, 0]);
                    continue;
                };
                covered += 1;
                assert_eq!(tile.coverage[i], 1.0);
                // Dominant axis of the face normal (cube faces are exact).
                let ax = [g.normal[0].abs(), g.normal[1].abs(), g.normal[2].abs()];
                let dom = if ax[0] >= ax[1] && ax[0] >= ax[2] {
                    0
                } else if ax[1] >= ax[2] {
                    1
                } else {
                    2
                };
                assert!(ax[dom] > 0.999, "cube face normal must be exact: {g:?}");
                faces_seen[dom * 2 + usize::from(g.normal[dom] < 0.0)] = true;
                // Expected: the dominant plane's coords through the test
                // pattern's cosine encoding (±3 for bilinear + byte
                // rounding; the encoding is wrap-continuous so integer
                // corner coordinates assert exactly like anywhere else).
                let plane = triplanar_planar_uvs(g.pos, 1.0)[dom];
                let [er, eg] = coord_expected(plane);
                let px = &tile.rgba[i * 4..i * 4 + 4];
                assert!(
                    (px[0] as f32 - er).abs() <= 3.0,
                    "texel ({x},{y}) R: got {} want ~{er} (plane {dom} {plane:?})",
                    px[0]
                );
                assert!(
                    (px[1] as f32 - eg).abs() <= 3.0,
                    "texel ({x},{y}) G: got {} want ~{eg} (plane {dom} {plane:?})",
                    px[1]
                );
                assert_eq!(px[2], 0);
                assert_eq!(px[3], 255);
            }
        }
        assert!(covered > 0);
        assert_eq!(faces_seen, [true; 6], "all six cube faces must rasterize");
    }

    /// Test (c): planar along X samples (y, z); Y and Z follow suit.
    #[test]
    fn planar_projects_onto_the_axis_plane() {
        let pos = [5.0, 0.25, 0.75];
        let uv = [0.1, 0.9];
        assert_eq!(
            projection_sample_2d(pos, uv, FillProjection::Planar { axis: ProjAxis::X }),
            [0.25, 0.75]
        );
        assert_eq!(
            projection_sample_2d(pos, uv, FillProjection::Planar { axis: ProjAxis::Y }),
            [5.0, 0.75]
        );
        assert_eq!(
            projection_sample_2d(pos, uv, FillProjection::Planar { axis: ProjAxis::Z }),
            [5.0, 0.25]
        );
        // UV projection is the identity regardless of position.
        assert_eq!(projection_sample_2d(pos, uv, FillProjection::Uv), uv);
    }

    /// Test (d): spherical — +X maps to theta = 0; poles map to v = 0 / 1.
    #[test]
    fn spherical_wraps_around_the_axes() {
        let at = |p: [f32; 3]| spherical_coords(p);
        let u = at([1.0, 0.0, 0.0]);
        assert!(near(u[0], 0.0) && near(u[1], 0.5), "+X: {u:?}");
        let z = at([0.0, 0.0, 1.0]);
        assert!(near(z[0], 0.25) && near(z[1], 0.5), "+Z: {z:?}");
        let nx = at([-1.0, 0.0, 0.0]);
        assert!(near(nx[0], 0.5) && near(nx[1], 0.5), "-X: {nx:?}");
        let top = at([0.0, 1.0, 0.0]);
        assert!(near(top[1], 0.0), "+Y pole: {top:?}");
        let bot = at([0.0, -1.0, 0.0]);
        assert!(near(bot[1], 1.0), "-Y pole: {bot:?}");
        // Radius-independent (direction only).
        let far = at([7.0, 0.0, 0.0]);
        assert!(near(far[0], 0.0) && near(far[1], 0.5), "{far:?}");
        // The origin has no direction: fixed fallback, never NaN.
        assert_eq!(at([0.0, 0.0, 0.0]), [0.0, 0.5]);
    }

    /// Test (e): cylindrical — height is the axis coordinate, angle wraps
    /// the other two.
    #[test]
    fn cylindrical_height_is_the_axis_coordinate() {
        let p = [0.3, 2.5, -0.4];
        let y = cylindrical_coords(p, ProjAxis::Y);
        assert!(near(y[1], 2.5), "{y:?}");
        assert!(
            near(y[0], wrap01(p[2].atan2(p[0]) / std::f32::consts::TAU)),
            "{y:?}"
        );
        let x = cylindrical_coords(p, ProjAxis::X);
        assert!(near(x[1], 0.3), "{x:?}");
        let z = cylindrical_coords(p, ProjAxis::Z);
        assert!(near(z[1], -0.4), "{z:?}");
        // +X around Y starts at angle 0, like spherical.
        assert!(near(
            cylindrical_coords([1.0, 9.0, 0.0], ProjAxis::Y)[0],
            0.0
        ));
    }

    /// Sampler unit tests: gradient endpoints/midpoint, noise determinism
    /// and range, image wrap + degenerate inputs.
    #[test]
    fn samplers_sample_their_documented_values() {
        let (a, b) = ([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.5]);
        let g = FillSource::Gradient { a, b, angle: 0.0 };
        assert_eq!(sample_source(&g, [0.0, 7.0]), a);
        let mid = sample_source(&g, [0.5, 0.0]);
        assert!(
            near(mid[0], 0.5) && near(mid[2], 0.5) && near(mid[3], 0.75),
            "{mid:?}"
        );
        // Tiling: t wraps past 1.
        assert_eq!(sample_source(&g, [1.0, 0.0]), a);
        assert_eq!(sample_source(&g, [-0.5, 0.0]), mid);

        let n = FillSource::Noise {
            seed: 42,
            scale: 3.0,
            octaves: 4,
        };
        let (v1, v2) = (
            sample_source(&n, [0.13, 0.71]),
            sample_source(&n, [0.13, 0.71]),
        );
        assert_eq!(v1, v2, "deterministic");
        assert!(
            (0.0..=1.0).contains(&v1[0]) && v1[1] == v1[0] && v1[2] == v1[0] && v1[3] == 1.0,
            "{v1:?}"
        );
        assert_ne!(
            sample_source(&n, [0.13, 0.71]),
            sample_source(&n, [0.9, 0.1]),
            "spatially varying"
        );
        // Octave clamp: 0 and 99 behave like 1 and 8 (no panic, in range).
        for octaves in [0, 99] {
            let s = FillSource::Noise {
                seed: 1,
                scale: 1.0,
                octaves,
            };
            let v = sample_source(&s, [0.5, 0.5]);
            assert!((0.0..=1.0).contains(&v[0]), "{v:?}");
        }

        // 1x1 image: every sample is that pixel (wrap-exact).
        let flat = FillSource::GraphImage {
            width: 1,
            height: 1,
            rgba: vec![10, 20, 30, 40],
        };
        assert_eq!(
            sample_source(&flat, [0.0, 0.0]),
            sample_source(&flat, [3.7, -2.0])
        );
        // Degenerate images sample transparent, never panic.
        for bad in [
            FillSource::GraphImage {
                width: 0,
                height: 0,
                rgba: vec![],
            },
            FillSource::GraphImage {
                width: 2,
                height: 2,
                rgba: vec![1, 2, 3],
            },
        ] {
            assert_eq!(sample_source(&bad, [0.25, 0.25]), [0.0, 0.0, 0.0, 0.0]);
        }
    }

    /// Test (h): the mask+fill composition — a polygon-masked fill paints
    /// only inside the lasso (same source+projection path, gated per
    /// texel before the byte is written).
    #[test]
    fn masked_fill_paints_only_inside_the_polygon() {
        let lasso = FillMask::polygon(vec![[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]]);
        let params = FillParams {
            source: FillSource::Solid {
                color: [1.0, 0.0, 0.0, 1.0],
            },
            projection: FillProjection::Uv,
            mask: Some(lasso),
        };
        let tile = evaluate_fill_tile(&full_uv_quad(), &params, 1001, 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let i = y * 16 + x;
                if x < 8 {
                    assert_eq!(&tile.rgba[i * 4..i * 4 + 4], &[255, 0, 0, 255], "inside ({x},{y})");
                    assert_eq!(tile.coverage[i], 1.0);
                } else {
                    assert_eq!(&tile.rgba[i * 4..i * 4 + 4], &[0, 0, 0, 0], "outside ({x},{y})");
                    assert_eq!(tile.coverage[i], 0.0);
                }
            }
        }
    }

    /// Test (i): the triangle mask paints only the picked face — every
    /// covered texel's geometry agrees it came from triangle 0, and
    /// triangle 1 stays transparent.
    #[test]
    fn triangle_mask_paints_only_the_picked_face() {
        let params = FillParams {
            source: FillSource::Solid {
                color: [0.0, 1.0, 0.0, 1.0],
            },
            projection: FillProjection::Uv,
            mask: Some(FillMask::triangle(0)),
        };
        let mesh = full_uv_quad();
        let tile = evaluate_fill_tile(&mesh, &params, 1001, 8, 8);
        let geom = rasterize_uv_geometry(&mesh, 1001, 8, 8);
        let mut covered = 0;
        for (i, g) in geom.iter().enumerate() {
            let g = g.expect("full coverage");
            if g.triangle == 0 {
                covered += 1;
                assert_eq!(&tile.rgba[i * 4..i * 4 + 4], &[0, 255, 0, 255]);
                assert_eq!(tile.coverage[i], 1.0);
            } else {
                assert_eq!(g.triangle, 1);
                assert_eq!(&tile.rgba[i * 4..i * 4 + 4], &[0, 0, 0, 0]);
                assert_eq!(tile.coverage[i], 0.0);
            }
        }
        assert!(covered > 0 && covered < 64, "a strict subset: {covered}");
    }

    /// Test (j): the island masks compose with non-UV projections — a
    /// planar fill gated by a UV-chunk paints the chunk's texels with the
    /// planar sample and nothing else.
    #[test]
    fn island_mask_composes_with_non_uv_projections() {
        let mesh = full_uv_quad();
        let chunk = FillMask::uv_chunk(&mesh, 0);
        let masked = FillParams {
            source: FillSource::Solid {
                color: [1.0, 1.0, 0.0, 1.0],
            },
            // The quad's shared vertices weld both triangles into one
            // UV island (same position AND same UVs), so the chunk is
            // the whole quad — full coverage, planar-sampled.
            projection: FillProjection::Planar { axis: ProjAxis::X },
            mask: Some(chunk),
        };
        let tile = evaluate_fill_tile(&mesh, &masked, 1001, 8, 8);
        assert!(tile.coverage.iter().all(|&c| c == 1.0));
        assert!(tile.rgba.chunks_exact(4).all(|px| px == [255, 255, 0, 255]));
        // And the empty-set degradation paints nothing, never panics.
        let empty = FillParams {
            mask: Some(FillMask::mesh_island(&mesh, 99)),
            ..masked
        };
        let tile = evaluate_fill_tile(&mesh, &empty, 1001, 8, 8);
        assert!(tile.coverage.iter().all(|&c| c == 0.0));
        assert!(tile.rgba.iter().all(|&b| b == 0));
    }

    /// The default params are unmasked, and the mask's heap counts toward
    /// the layer's memory accounting.
    #[test]
    fn default_params_are_unmasked_and_masks_count_bytes() {
        assert_eq!(FillParams::default().mask, None);
        let plain = solid_params([1.0, 1.0, 1.0, 1.0]).memory_bytes();
        let masked = FillParams {
            mask: Some(FillMask::polygon(vec![[0.0; 2]; 10])),
            ..solid_params([1.0, 1.0, 1.0, 1.0])
        }
        .memory_bytes();
        assert!(masked > plain, "{masked} vs {plain}");
    }

    /// Malformed meshes degrade to uncovered texels, never panics — the
    /// crate's no-panic rule applied to the fill path.
    #[test]
    fn malformed_meshes_rasterize_to_nothing() {
        let mut mesh = full_uv_quad();
        mesh.indices = vec![0, 1, 99];
        let geom = rasterize_uv_geometry(&mesh, 1001, 8, 8);
        assert!(geom.iter().all(|g| g.is_none()));
        let tile = evaluate_fill_tile(&mesh, &solid_params([1.0, 1.0, 1.0, 1.0]), 1001, 8, 8);
        assert!(tile.coverage.iter().all(|&c| c == 0.0));

        let empty = MeshData::default();
        assert!(rasterize_uv_geometry(&empty, 1001, 8, 8)
            .iter()
            .all(|g| g.is_none()));
        let zero = evaluate_fill_tile(
            &full_uv_quad(),
            &solid_params([1.0, 0.0, 0.0, 1.0]),
            1001,
            0,
            8,
        );
        assert!(zero.rgba.is_empty() && zero.coverage.is_empty());
    }
}
