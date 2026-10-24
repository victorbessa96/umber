//! Symmetry (§2's P1 row): mirror (axis + offset + manipulator) and
//! radial (count, flip U/V), for brush dabs and fill-layer coverage.
//!
//! The model is pure: [`SymmetrySettings`] → a stable list of affine
//! [`SymTransform`]s in tile-local UV (`[0, 1]²`, V up), identity first.
//! Every consumer stamps the SAME primitive at each transformed position:
//!
//! * Paint dabs (`umber-app`'s `PaintState`): the conditioned dabs of an
//!   original event are expanded AFTER the stroke conditioner — siblings
//!   are exact images of the smoothed path, never fed back through the
//!   one-euro/spacing state. The dab shape is radial, so only the position
//!   moves.
//! * Retouch dabs (smudge/clone, `umber-app`'s `RetouchStroke`): each
//!   sibling keeps its own previous-dab chain, so a smudge sibling's motion
//!   vector is the transformed motion (mirrored with the point). Clone
//!   siblings sample the transformed source point.
//! * Fill coverage ([`symmetrize_coverage`]): the fill-layer hook. No fill
//!   op exists yet (`LayerKind::Fill` is a tag; §2's fill-projection row is
//!   unimplemented), so this is the pure function the fill op calls on its
//!   projected coverage — the union of the coverage's symmetric images.
//!
//! Texel space: the paint targets store rows top-down (`y = (1 - v) * s`),
//! so a UV transform is conjugated through that flip before it touches
//! texel positions ([`SymTransform::to_texel_space`]): a V-mirror at offset
//! `o` reflects about `y = (1 - o) * s`, and a UV rotation turns the other
//! way in texel space.
//!
//! Combination: with both on, the radial set is built first and every
//! radial copy is then mirrored, so the copy count is
//! `count * (flip copies) * 2`, minus coincident duplicates.

use serde::{Deserialize, Serialize};
use umber_gpu::paint::Dab;

/// The radial center, tile-local UV (v1: fixed at the tile center; a
/// movable center with its own manipulator is a named follow-up).
pub const RADIAL_CENTER: (f32, f32) = (0.5, 0.5);

/// The radial copy-count range (`count` is clamped into it).
pub const RADIAL_COUNT_MIN: u32 = 2;
pub const RADIAL_COUNT_MAX: u32 = 16;

/// Two symmetric positions closer than this (UV units) are one stamp: a
/// dab exactly on a mirror axis must not stamp twice (double alpha).
pub const COINCIDENT_EPS_UV: f32 = 1e-5;

/// Which UV coordinate the mirror reflects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MirrorAxis {
    /// Reflect U about the vertical line `u = offset`.
    #[default]
    X,
    /// Reflect V about the horizontal line `v = offset`.
    Y,
}

/// Mirror symmetry: one reflected copy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MirrorSettings {
    pub enabled: bool,
    pub axis: MirrorAxis,
    /// The mirror line's position along the reflected coordinate,
    /// tile-local UV (0.5 = the tile's center line). The UV view's
    /// manipulator drags this.
    pub offset: f32,
}

impl Default for MirrorSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            axis: MirrorAxis::X,
            offset: 0.5,
        }
    }
}

/// Radial symmetry: `count` rotated copies about [`RADIAL_CENTER`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RadialSettings {
    pub enabled: bool,
    /// Copies including the original, clamped to
    /// `RADIAL_COUNT_MIN..=RADIAL_COUNT_MAX`.
    pub count: u32,
    /// Also stamp every rotated copy reflected in U (`u ↦ 2c − u`) — the
    /// kaleidoscope (dihedral) variant.
    pub flip_u: bool,
    /// Also stamp every rotated copy reflected in V (`v ↦ 2c − v`).
    pub flip_v: bool,
}

impl Default for RadialSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            count: 4,
            flip_u: false,
            flip_v: false,
        }
    }
}

/// The symmetry settings. The `.umber` project CARRIES these since the
/// 2026-10-24 persistence kill (core's `ProjectSymmetry` mirror + the
/// app's boundary conversions); the `.umberbrush` preset deliberately
/// does NOT (a preset is a brush shape, not a tool state — Painter's
/// own symmetry is tool-level). The paint session adopts the settings
/// between strokes (`sync_symmetry`).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct SymmetrySettings {
    pub mirror: MirrorSettings,
    pub radial: RadialSettings,
}

impl SymmetrySettings {
    /// Whether any symmetry is on (off = the caller skips expansion
    /// entirely: the zero-cost, byte-identical path).
    pub fn is_active(&self) -> bool {
        self.mirror.enabled || self.radial.enabled
    }
}

/// An affine map `p ↦ m · p + t` (row-major `m`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SymTransform {
    pub m: [[f32; 2]; 2],
    pub t: [f32; 2],
}

impl SymTransform {
    pub const IDENTITY: Self = Self {
        m: [[1.0, 0.0], [0.0, 1.0]],
        t: [0.0, 0.0],
    };

    /// The image of point `p`.
    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        let [x, y] = self.apply_vector(p);
        [x + self.t[0], y + self.t[1]]
    }

    /// The image of direction `v` (the linear part only: motion vectors).
    pub fn apply_vector(&self, v: [f32; 2]) -> [f32; 2] {
        [
            self.m[0][0] * v[0] + self.m[0][1] * v[1],
            self.m[1][0] * v[0] + self.m[1][1] * v[1],
        ]
    }

    /// `self ∘ other` (apply `other` first).
    #[must_use]
    pub fn compose(&self, other: &Self) -> Self {
        let a = &self.m;
        let b = &other.m;
        Self {
            m: [
                [
                    a[0][0] * b[0][0] + a[0][1] * b[1][0],
                    a[0][0] * b[0][1] + a[0][1] * b[1][1],
                ],
                [
                    a[1][0] * b[0][0] + a[1][1] * b[1][0],
                    a[1][0] * b[0][1] + a[1][1] * b[1][1],
                ],
            ],
            t: self.apply(other.t),
        }
    }

    /// The inverse map. Every symmetry transform is a rotation or a
    /// reflection (determinant ±1), so it always exists.
    #[must_use]
    pub fn inverse(&self) -> Self {
        let m = &self.m;
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        let inv = [
            [m[1][1] / det, -m[0][1] / det],
            [-m[1][0] / det, m[0][0] / det],
        ];
        let lin = Self {
            m: inv,
            t: [0.0, 0.0],
        };
        let t = lin.apply_vector(self.t);
        Self {
            m: inv,
            t: [-t[0], -t[1]],
        }
    }

    /// This UV-space transform conjugated into tile-local texel space
    /// (`x = u * s`, `y = (1 - v) * s`, `s` = texels per UV unit): the
    /// map `F ∘ self ∘ F⁻¹`. The scale cancels in the linear part; the V
    /// flip negates its off-diagonal terms.
    #[must_use]
    pub fn to_texel_space(&self, texels_per_uv: f32) -> Self {
        let s = texels_per_uv;
        let to_uv = |p: [f32; 2]| [p[0] / s, 1.0 - p[1] / s];
        let to_texel = |uv: [f32; 2]| [uv[0] * s, (1.0 - uv[1]) * s];
        Self {
            m: [[self.m[0][0], -self.m[0][1]], [-self.m[1][0], self.m[1][1]]],
            t: to_texel(self.apply(to_uv([0.0, 0.0]))),
        }
    }
}

/// Reflection `u ↦ 2c − u`.
fn reflect_u(c: f32) -> SymTransform {
    SymTransform {
        m: [[-1.0, 0.0], [0.0, 1.0]],
        t: [2.0 * c, 0.0],
    }
}

/// Reflection `v ↦ 2c − v`.
fn reflect_v(c: f32) -> SymTransform {
    SymTransform {
        m: [[1.0, 0.0], [0.0, -1.0]],
        t: [0.0, 2.0 * c],
    }
}

/// Counter-clockwise rotation (UV: u right, v up) by `k / n` turns about
/// `c`. Quarter turns are built exactly (no `sin`/`cos` residue), so the
/// common 2/4-fold cases stamp on exact positions.
fn rotation(k: u32, n: u32, c: (f32, f32)) -> SymTransform {
    let (cos, sin) = match (4 * k) % (4 * n) {
        0 => (1.0, 0.0),
        q if q == n => (0.0, 1.0),
        q if q == 2 * n => (-1.0, 0.0),
        q if q == 3 * n => (0.0, -1.0),
        _ => {
            let a = std::f32::consts::TAU * k as f32 / n as f32;
            (a.cos(), a.sin())
        }
    };
    let m = [[cos, -sin], [sin, cos]];
    // p' = R (p − c) + c  ⇒  t = c − R c.
    let rc = [m[0][0] * c.0 + m[0][1] * c.1, m[1][0] * c.0 + m[1][1] * c.1];
    SymTransform {
        m,
        t: [c.0 - rc[0], c.1 - rc[1]],
    }
}

/// The settings' transforms in tile-local UV, identity first, in a STABLE
/// order (callers key per-sibling state by index): the radial copies
/// (rotation k = 0..count, then the flip-U images, then the flip-V
/// images), then — with the mirror on — each of those mirrored. Off
/// settings yield `[IDENTITY]`. Transforms are not deduplicated (two can
/// coincide, e.g. flip U and flip V at count 2); positions are, at stamp
/// time ([`sibling_positions`]).
pub fn symmetric_transforms(settings: &SymmetrySettings) -> Vec<SymTransform> {
    let mut out = vec![SymTransform::IDENTITY];
    let radial = &settings.radial;
    if radial.enabled {
        let n = radial.count.clamp(RADIAL_COUNT_MIN, RADIAL_COUNT_MAX);
        let rotations: Vec<SymTransform> = (0..n).map(|k| rotation(k, n, RADIAL_CENTER)).collect();
        out = rotations.clone();
        if radial.flip_u {
            let f = reflect_u(RADIAL_CENTER.0);
            out.extend(rotations.iter().map(|r| f.compose(r)));
        }
        if radial.flip_v {
            let f = reflect_v(RADIAL_CENTER.1);
            out.extend(rotations.iter().map(|r| f.compose(r)));
        }
    }
    let mirror = &settings.mirror;
    if mirror.enabled {
        let f = match mirror.axis {
            MirrorAxis::X => reflect_u(mirror.offset),
            MirrorAxis::Y => reflect_v(mirror.offset),
        };
        let mirrored: Vec<SymTransform> = out.iter().map(|t| f.compose(t)).collect();
        out.extend(mirrored);
    }
    out
}

/// Every symmetric image of `uv` (tile-local UV): the identity first, then
/// the mirrored/rotated candidates, coincident positions dropped — each a
/// position the caller stamps.
pub fn symmetric_points(uv: (f32, f32), settings: &SymmetrySettings) -> Vec<(f32, f32)> {
    let transforms = symmetric_transforms(settings);
    sibling_positions([uv.0, uv.1], &transforms, COINCIDENT_EPS_UV)
        .map(|(_, p)| (p[0], p[1]))
        .collect()
}

/// `(sibling index, position)` for each transform's image of `pos`,
/// skipping images within `eps` of an earlier sibling's (an on-axis dab
/// stamps once). The index is the transform's — stable across dabs even
/// when a duplicate is skipped, so per-sibling state never shifts.
pub fn sibling_positions<'a>(
    pos: [f32; 2],
    transforms: &'a [SymTransform],
    eps: f32,
) -> impl Iterator<Item = (usize, [f32; 2])> + 'a {
    let images: Vec<[f32; 2]> = transforms.iter().map(|t| t.apply(pos)).collect();
    (0..images.len()).filter_map(move |k| {
        let p = images[k];
        let dup = images[..k]
            .iter()
            .any(|q| (q[0] - p[0]).abs() <= eps && (q[1] - p[1]).abs() <= eps);
        (!dup).then_some((k, p))
    })
}

/// The settings' transforms in tile-local texel space (see
/// [`SymTransform::to_texel_space`]), identity first.
pub fn texel_transforms(settings: &SymmetrySettings, texels_per_uv: f32) -> Vec<SymTransform> {
    symmetric_transforms(settings)
        .iter()
        .map(|t| t.to_texel_space(texels_per_uv))
        .collect()
}

/// Two sibling dab positions closer than this (texels) are one stamp.
pub const COINCIDENT_EPS_TEXELS: f32 = 1e-3;

/// The brush hook: every dab followed by its symmetric siblings — the same
/// stamp (radius, alpha, color, hardness) at each transformed position,
/// dab-major so stroke order holds per sibling. `texel_transforms` comes
/// from [`texel_transforms`]; a lone identity returns the dabs unchanged.
pub fn expand_dabs(dabs: &[Dab], texel_transforms: &[SymTransform]) -> Vec<Dab> {
    let mut out = Vec::with_capacity(dabs.len() * texel_transforms.len());
    for dab in dabs {
        for (_, pos) in sibling_positions(dab.pos, texel_transforms, COINCIDENT_EPS_TEXELS) {
            let mut sibling = *dab;
            sibling.pos = pos;
            out.push(sibling);
        }
    }
    out
}

/// The fill-layer hook: the union (per-texel max) of `coverage`'s images
/// under `texel_transforms` — a `width`x`height` row-major coverage mask
/// (0..=1) in tile-local texels, as a fill projection produces it. Each
/// output texel center samples every inverse-transformed position
/// (nearest texel; off-image samples contribute nothing). A mirror or a
/// quarter-turn maps texel centers onto texel centers, so those cases are
/// exact. `None` when `coverage` is not `width * height` long.
pub fn symmetrize_coverage(
    coverage: &[f32],
    width: u32,
    height: u32,
    texel_transforms: &[SymTransform],
) -> Option<Vec<f32>> {
    let (w, h) = (width as usize, height as usize);
    if coverage.len() != w * h {
        return None;
    }
    let inverses: Vec<SymTransform> = texel_transforms.iter().map(SymTransform::inverse).collect();
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let center = [x as f32 + 0.5, y as f32 + 0.5];
            out[y * w + x] = inverses
                .iter()
                .filter_map(|inv| {
                    let [sx, sy] = inv.apply(center);
                    let (sx, sy) = (sx.floor(), sy.floor());
                    let inside = sx >= 0.0 && sy >= 0.0 && (sx as usize) < w && (sy as usize) < h;
                    inside.then(|| coverage[sy as usize * w + sx as usize])
                })
                .fold(0.0, f32::max);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    fn near(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < EPS && (a.1 - b.1).abs() < EPS
    }

    fn contains(points: &[(f32, f32)], p: (f32, f32)) -> bool {
        points.iter().any(|&q| near(q, p))
    }

    fn mirror(axis: MirrorAxis, offset: f32) -> SymmetrySettings {
        SymmetrySettings {
            mirror: MirrorSettings {
                enabled: true,
                axis,
                offset,
            },
            ..SymmetrySettings::default()
        }
    }

    fn radial(count: u32, flip_u: bool, flip_v: bool) -> SymmetrySettings {
        SymmetrySettings {
            radial: RadialSettings {
                enabled: true,
                count,
                flip_u,
                flip_v,
            },
            ..SymmetrySettings::default()
        }
    }

    #[test]
    fn off_is_the_identity_only() {
        let off = SymmetrySettings::default();
        assert!(!off.is_active());
        assert_eq!(symmetric_transforms(&off), vec![SymTransform::IDENTITY]);
        assert_eq!(symmetric_points((0.3, 0.7), &off), vec![(0.3, 0.7)]);
    }

    #[test]
    fn mirror_x_at_half_reflects_u() {
        let pts = symmetric_points((0.25, 0.5), &mirror(MirrorAxis::X, 0.5));
        assert_eq!(pts.len(), 2);
        assert!(near(pts[0], (0.25, 0.5)), "identity first: {pts:?}");
        assert!(near(pts[1], (0.75, 0.5)), "{pts:?}");
    }

    #[test]
    fn mirror_honors_axis_and_offset() {
        // Off-center offsets pin the reflection line (0.5 would hide an
        // offset/axis mix-up).
        let x = symmetric_points((0.1, 0.3), &mirror(MirrorAxis::X, 0.25));
        assert!(near(x[1], (0.4, 0.3)), "{x:?}");
        let y = symmetric_points((0.5, 0.1), &mirror(MirrorAxis::Y, 0.25));
        assert!(near(y[1], (0.5, 0.4)), "{y:?}");
    }

    #[test]
    fn on_axis_points_stamp_once() {
        let pts = symmetric_points((0.5, 0.2), &mirror(MirrorAxis::X, 0.5));
        assert_eq!(pts, vec![(0.5, 0.2)]);
    }

    #[test]
    fn radial_four_gives_the_four_quarter_turns() {
        let pts = symmetric_points((0.75, 0.5), &radial(4, false, false));
        assert_eq!(pts.len(), 4, "{pts:?}");
        // Counter-clockwise in UV (u right, v up), identity first.
        assert!(near(pts[0], (0.75, 0.5)));
        assert!(near(pts[1], (0.5, 0.75)));
        assert!(near(pts[2], (0.25, 0.5)));
        assert!(near(pts[3], (0.5, 0.25)));
        // Quarter turns are exact (no sin/cos residue).
        assert_eq!(pts[2], (0.25, 0.5));
    }

    #[test]
    fn radial_count_is_clamped() {
        assert_eq!(symmetric_transforms(&radial(0, false, false)).len(), 2);
        assert_eq!(symmetric_transforms(&radial(99, false, false)).len(), 16);
        // A generic point keeps all six copies of a 6-fold set, each at
        // the same distance from the center.
        let pts = symmetric_points((0.8, 0.6), &radial(6, false, false));
        assert_eq!(pts.len(), 6);
        let d = |p: (f32, f32)| ((p.0 - 0.5).powi(2) + (p.1 - 0.5).powi(2)).sqrt();
        for p in &pts {
            assert!((d(*p) - d(pts[0])).abs() < EPS, "{pts:?}");
        }
    }

    #[test]
    fn radial_flips_add_the_reflected_copies() {
        // Count 2: the original and its half-turn.
        let p = (0.75, 0.6);
        let plain = symmetric_points(p, &radial(2, false, false));
        assert_eq!(plain.len(), 2);
        assert!(contains(&plain, (0.25, 0.4)));
        // flip_u swaps U about the center for every copy.
        let fu = symmetric_points(p, &radial(2, true, false));
        assert_eq!(fu.len(), 4, "{fu:?}");
        assert!(contains(&fu, (0.25, 0.6)));
        assert!(contains(&fu, (0.75, 0.4)));
        // flip_v swaps V; at count 2 its images coincide with flip_u's
        // (a U-flip of the half-turn is a V-flip), so both together still
        // stamp four distinct positions.
        let fv = symmetric_points(p, &radial(2, false, true));
        assert_eq!(fv.len(), 4, "{fv:?}");
        assert!(contains(&fv, (0.75, 0.4)));
        assert!(contains(&fv, (0.25, 0.6)));
        let both = symmetric_points(p, &radial(2, true, true));
        assert_eq!(both.len(), 4, "{both:?}");
        // Count 3 with flip_u: six distinct (the dihedral D3 orbit).
        assert_eq!(symmetric_points(p, &radial(3, true, false)).len(), 6);
    }

    #[test]
    fn mirror_and_radial_combine() {
        let settings = SymmetrySettings {
            mirror: MirrorSettings {
                enabled: true,
                axis: MirrorAxis::X,
                offset: 0.5,
            },
            radial: RadialSettings {
                enabled: true,
                count: 3,
                flip_u: false,
                flip_v: false,
            },
        };
        assert_eq!(symmetric_transforms(&settings).len(), 6);
        let pts = symmetric_points((0.7, 0.55), &settings);
        assert_eq!(pts.len(), 6);
        assert!(contains(&pts, (0.3, 0.55)), "the mirror of the original");
    }

    #[test]
    fn sibling_indices_stay_stable_across_dedup() {
        let ts = symmetric_transforms(&radial(2, true, true));
        assert_eq!(ts.len(), 6);
        // A generic point: identity, half-turn, flip-U ×2, flip-V ×2 —
        // the flip-V images duplicate flip-U's, so indices 4 and 5 drop
        // while 0..=3 keep their numbers.
        let ks: Vec<usize> = sibling_positions([0.75, 0.6], &ts, EPS)
            .map(|(k, _)| k)
            .collect();
        assert_eq!(ks, vec![0, 1, 2, 3]);
    }

    #[test]
    fn texel_conjugation_commutes_with_the_uv_map() {
        let s = 512.0;
        let f = |uv: [f32; 2]| [uv[0] * s, (1.0 - uv[1]) * s];
        let settings = [
            mirror(MirrorAxis::X, 0.25),
            mirror(MirrorAxis::Y, 0.25),
            radial(4, true, false),
            radial(5, false, true),
        ];
        for settings in &settings {
            for t in symmetric_transforms(settings) {
                let tt = t.to_texel_space(s);
                for uv in [[0.5, 0.1], [0.13, 0.77], [0.9, 0.42]] {
                    let a = tt.apply(f(uv));
                    let b = f(t.apply(uv));
                    assert!(
                        (a[0] - b[0]).abs() < 1e-2 && (a[1] - b[1]).abs() < 1e-2,
                        "T_texel(F(uv)) != F(T(uv)) for {settings:?} at {uv:?}: {a:?} vs {b:?}"
                    );
                }
            }
        }
        // The V-mirror at 0.25 reflects about y = (1 - 0.25) * s, not 0.25 * s.
        let ty = texel_transforms(&mirror(MirrorAxis::Y, 0.25), s)[1];
        assert_eq!(ty.apply([10.0, 0.75 * s]), [10.0, 0.75 * s]);
    }

    #[test]
    fn inverse_round_trips() {
        for t in symmetric_transforms(&radial(5, true, false)) {
            let p = [0.31, 0.82];
            let back = t.inverse().apply(t.apply(p));
            assert!((back[0] - p[0]).abs() < EPS && (back[1] - p[1]).abs() < EPS);
        }
    }

    /// Test (b), headless: one event's dabs stage N siblings each with
    /// symmetry on, exactly the input with it off; the stamp is identical,
    /// only the position moves.
    #[test]
    fn expand_dabs_stages_one_stamp_per_sibling() {
        let s = 512.0;
        let dabs = [
            Dab::new([100.0, 200.0], 24.0, [0.85, 0.2, 0.1, 1.0], 0.8, 0.5),
            Dab::new([104.0, 201.0], 24.0, [0.85, 0.2, 0.1, 1.0], 0.6, 0.5),
        ];
        let off = expand_dabs(&dabs, &texel_transforms(&SymmetrySettings::default(), s));
        assert_eq!(off.len(), 2);
        assert_eq!(off[1].pos, dabs[1].pos);

        let m = expand_dabs(&dabs, &texel_transforms(&mirror(MirrorAxis::X, 0.5), s));
        assert_eq!(m.len(), 4, "two dabs x (original + mirror)");
        // Dab-major: original, its mirror, next original, its mirror.
        assert_eq!(m[0].pos, [100.0, 200.0]);
        assert_eq!(m[1].pos, [412.0, 200.0]);
        assert_eq!(m[3].pos, [408.0, 201.0]);
        assert_eq!(m[1].radius, dabs[0].radius);
        assert_eq!(m[1].alpha, dabs[0].alpha);
        assert_eq!(m[1].hardness, dabs[0].hardness);
        assert_eq!(m[1].color, dabs[0].color);

        let r = expand_dabs(&dabs, &texel_transforms(&radial(6, false, false), s));
        assert_eq!(r.len(), 12, "two dabs x six radial copies");
        // An on-axis dab stamps once even with the mirror on.
        let on_axis = [Dab::new([256.0, 40.0], 24.0, [1.0; 4], 1.0, 1.0)];
        let one = expand_dabs(&on_axis, &texel_transforms(&mirror(MirrorAxis::X, 0.5), s));
        assert_eq!(one.len(), 1);
    }

    /// Test (c): the fill coverage. A fill covering the left quarter,
    /// mirrored at U = 0.5, covers the right quarter too and nothing in
    /// between; radial ×4 turns a right-edge strip into a frame.
    #[test]
    fn fill_coverage_is_the_union_of_its_symmetric_images() {
        let (w, h) = (16u32, 16u32);
        let n = (w * h) as usize;
        let left_quarter: Vec<f32> = (0..n)
            .map(|i| if (i % w as usize) < 4 { 1.0 } else { 0.0 })
            .collect();
        let ts = texel_transforms(&mirror(MirrorAxis::X, 0.5), w as f32);
        let out = symmetrize_coverage(&left_quarter, w, h, &ts).expect("shape matches");
        for y in 0..h as usize {
            for x in 0..w as usize {
                let want = if !(4..12).contains(&x) { 1.0 } else { 0.0 };
                assert_eq!(out[y * 16 + x], want, "texel ({x},{y})");
            }
        }
        // Off: the coverage passes through untouched.
        let id = texel_transforms(&SymmetrySettings::default(), w as f32);
        assert_eq!(
            symmetrize_coverage(&left_quarter, w, h, &id).as_deref(),
            Some(&left_quarter[..])
        );
        // Radial 4: the left strip's quarter turns cover all four edges.
        let ts = texel_transforms(&radial(4, false, false), w as f32);
        let out = symmetrize_coverage(&left_quarter, w, h, &ts).expect("shape matches");
        let at = |x: usize, y: usize| out[y * 16 + x];
        assert_eq!(at(1, 8), 1.0, "left");
        assert_eq!(at(14, 8), 1.0, "right");
        assert_eq!(at(8, 1), 1.0, "top");
        assert_eq!(at(8, 14), 1.0, "bottom");
        assert_eq!(at(8, 8), 0.0, "center stays uncovered");
        // A shape mismatch is rejected, not indexed out of bounds.
        assert_eq!(symmetrize_coverage(&left_quarter, 8, 8, &ts), None);
    }
}
