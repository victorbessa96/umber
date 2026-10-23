//! Per-vertex tangent frames, the mikkTSpace way — the foundation the
//! requirements row 79 named follow-up builds on (true per-vertex
//! tangents for the height→normal gradient; today's viewport uses the
//! per-facet screen-space derivative frame — correct per-facet, not
//! across a curved mesh).
//!
//! umber's importer output is PER-CORNER EXPANDED: each triangle owns
//! three vertex slots and shared corners are bit-identical duplicates,
//! so index-sharing accumulation (the textbook mikkTSpace input
//! shape) would never smooth anything here. This module welds corners
//! into groups by the bit-exact (position, normal, uv) tuple first —
//! the graphics-standard tangent-seam rule: corners weld only when
//! every attribute agrees, so a UV seam legitimately splits groups and
//! keeps per-island frames — then accumulates each group's tangent
//! sums across every triangle that touches it.
//!
//! Never panics on untrusted input: out-of-range indices and triangles
//! referencing missing attribute slots are skipped, a remainder after
//! `% 3` is ignored, degenerate (zero-UV-area) triangles contribute
//! nothing, and a group with no usable contribution takes a
//! deterministic fallback frame (the normal's least-dominant-axis
//! perpendicular; a zero normal takes `[1, 0, 0]`).
//!
//! The future consumer (the post-recovery renderer wiring — see the
//! kill report): upload `tangent` + `handedness` as vertex attributes
//! (`B = handedness · cross(N, T)`) and rebuild the height gradient
//! on the interpolated frame instead of the per-facet derivative
//! frame. This module is consumed by its own tests until then.

use std::collections::HashMap;

/// One vertex slot's tangent frame: the normal-orthonormalized tangent
/// plus the bitangent handedness — the standard compact encoding
/// (`B = handedness · cross(N, T)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TangentFrame {
    /// Unit length, perpendicular to the vertex normal.
    pub tangent: [f32; 3],
    /// ±1: the sign of `dot(cross(N, T), B_sum)` — +1 is right-handed.
    pub handedness: f32,
}

/// The per-triangle UV-derivative tangent/bitangent (Lengyel):
/// `e1 = du1·T + dv1·B` and `e2 = du2·T + dv2·B` solved for T and B.
/// `None` when the UV triangle is degenerate (zero cross product).
fn face_tangent(
    e1: [f32; 3],
    e2: [f32; 3],
    du1: f32,
    dv1: f32,
    du2: f32,
    dv2: f32,
) -> Option<([f32; 3], [f32; 3])> {
    let det = du1 * dv2 - du2 * dv1;
    // A degenerate UV triangle (all three uvs collinear or coincident)
    // has no tangent plane to contribute. The threshold is absolute:
    // real importer UVs sit far above it, and a relative epsilon would
    // need a reference scale this pure fn does not have.
    if det.abs() < 1e-10 {
        return None;
    }
    let r = 1.0 / det;
    let t = [
        (dv2 * e1[0] - dv1 * e2[0]) * r,
        (dv2 * e1[1] - dv1 * e2[1]) * r,
        (dv2 * e1[2] - dv1 * e2[2]) * r,
    ];
    let b = [
        (du1 * e2[0] - du2 * e1[0]) * r,
        (du1 * e2[1] - du2 * e1[1]) * r,
        (du1 * e2[2] - du2 * e1[2]) * r,
    ];
    Some((t, b))
}

/// The deterministic fallback frame for a group with no usable tangent
/// contribution: a unit vector perpendicular to `normal` built from its
/// least-dominant axis (maximizing the cross product's length, so it
/// never degenerates for a nonzero normal), handedness +1. A zero
/// normal cannot define a perpendicular, so it takes `[1, 0, 0]`.
fn fallback_frame(normal: [f32; 3]) -> TangentFrame {
    let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if len < 1e-20 {
        return TangentFrame {
            tangent: [1.0, 0.0, 0.0],
            handedness: 1.0,
        };
    }
    // The least-dominant axis (ties resolve to the lowest index —
    // deterministic for every input).
    let abs = [normal[0].abs(), normal[1].abs(), normal[2].abs()];
    let helper = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let c = cross(normal, helper);
    let clen = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    TangentFrame {
        tangent: [c[0] / clen, c[1] / clen, c[2] / clen],
        handedness: 1.0,
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Computes one [`TangentFrame`] per index slot of a triangle list.
///
/// * `positions` / `normals` / `uvs` are parallel per-slot attribute
///   arrays; `indices` is a triangle list (every 3 consecutive entries
///   one triangle; a remainder is ignored).
/// * Slots weld into accumulation groups by the bit-exact
///   (position, normal, uv) tuple — duplicated corners from the
///   per-corner expansion weld, UV-seam corners legitimately do not.
/// * Returns exactly `min(positions.len(), normals.len(), uvs.len())`
///   frames; a triangle referencing a slot at or beyond that count is
///   skipped (never a panic on untrusted input).
pub fn vertex_tangent_frames(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
) -> Vec<TangentFrame> {
    let slot_count = positions
        .len()
        .min(normals.len())
        .min(uvs.len())
        .min(u32::MAX as usize);
    // Weld groups: first-encounter order keeps the group ids
    // deterministic for a given input.
    let mut group_of = vec![u32::MAX; slot_count];
    let mut group_key: HashMap<[u8; 32], u32> = HashMap::new();
    let mut group_normals: Vec<[f32; 3]> = Vec::new();
    for slot in 0..slot_count {
        let mut key = [0u8; 32];
        key[0..4].copy_from_slice(&positions[slot][0].to_bits().to_le_bytes());
        key[4..8].copy_from_slice(&positions[slot][1].to_bits().to_le_bytes());
        key[8..12].copy_from_slice(&positions[slot][2].to_bits().to_le_bytes());
        key[12..16].copy_from_slice(&normals[slot][0].to_bits().to_le_bytes());
        key[16..20].copy_from_slice(&normals[slot][1].to_bits().to_le_bytes());
        key[20..24].copy_from_slice(&normals[slot][2].to_bits().to_le_bytes());
        key[24..28].copy_from_slice(&uvs[slot][0].to_bits().to_le_bytes());
        key[28..32].copy_from_slice(&uvs[slot][1].to_bits().to_le_bytes());
        let next = group_normals.len() as u32;
        let id = *group_key.entry(key).or_insert(next);
        if id == next {
            group_normals.push(normals[slot]);
        }
        group_of[slot] = id;
    }
    let group_count = group_normals.len();
    let mut t_sum = vec![[0f32; 3]; group_count];
    let mut b_sum = vec![[0f32; 3]; group_count];

    let triangles = indices.len() / 3;
    for tri in 0..triangles {
        let i0 = indices[tri * 3] as usize;
        let i1 = indices[tri * 3 + 1] as usize;
        let i2 = indices[tri * 3 + 2] as usize;
        if i0 >= slot_count || i1 >= slot_count || i2 >= slot_count {
            continue;
        }
        let p0 = positions[i0];
        let p1 = positions[i1];
        let p2 = positions[i2];
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let uv0 = uvs[i0];
        let uv1 = uvs[i1];
        let uv2 = uvs[i2];
        let Some((t_face, b_face)) = face_tangent(
            e1,
            e2,
            uv1[0] - uv0[0],
            uv1[1] - uv0[1],
            uv2[0] - uv0[0],
            uv2[1] - uv0[1],
        ) else {
            continue;
        };
        // Unweighted per-corner accumulation (the mikkTSpace shape):
        // every touched group takes the face's full tangent and
        // bitangent into its sums.
        for slot in [i0, i1, i2] {
            let g = group_of[slot] as usize;
            t_sum[g][0] += t_face[0];
            t_sum[g][1] += t_face[1];
            t_sum[g][2] += t_face[2];
            b_sum[g][0] += b_face[0];
            b_sum[g][1] += b_face[1];
            b_sum[g][2] += b_face[2];
        }
    }

    // Per-group finalize: Gram-Schmidt against the group's normal,
    // handedness from the bitangent sum, deterministic fallbacks.
    let mut group_frame = Vec::with_capacity(group_count);
    for g in 0..group_count {
        let n = group_normals[g];
        let t = t_sum[g];
        // Gram-Schmidt: strip the normal component, then normalize.
        let ndot = dot(n, t);
        let mut gs = [t[0] - n[0] * ndot, t[1] - n[1] * ndot, t[2] - n[2] * ndot];
        let len = (gs[0] * gs[0] + gs[1] * gs[1] + gs[2] * gs[2]).sqrt();
        if len < 1e-20 {
            group_frame.push(fallback_frame(n));
            continue;
        }
        gs = [gs[0] / len, gs[1] / len, gs[2] / len];
        let c = cross(n, gs);
        let handedness = if dot(c, b_sum[g]) < 0.0 { -1.0 } else { 1.0 };
        group_frame.push(TangentFrame {
            tangent: gs,
            handedness,
        });
    }

    (0..slot_count)
        .map(|slot| group_frame[group_of[slot] as usize])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flat quad in the XY plane, u = x and v = y, fan-triangulated
    /// into TWO triangles with the shared corners bit-identically
    /// duplicated — the per-corner expansion's exact shape. All frames
    /// must come out `tangent = +X`, `handedness = +1` (the identity UV
    /// map's frame), which requires the weld: without it, each slot
    /// would see only its own triangle and the shared corners could
    /// still agree — so the seam test below is the one that really
    /// discriminates. This one pins the baseline.
    #[test]
    fn flat_quad_welds_into_one_consistent_frame() {
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let normals = [[0.0, 0.0, 1.0]; 6];
        let uvs = [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ];
        let indices: Vec<u32> = (0..6).collect();
        let frames = vertex_tangent_frames(&positions, &normals, &uvs, &indices);
        assert_eq!(frames.len(), 6);
        for (slot, frame) in frames.iter().enumerate() {
            assert!(
                (frame.tangent[0] - 1.0).abs() < 1e-6
                    && frame.tangent[1].abs() < 1e-6
                    && frame.tangent[2].abs() < 1e-6,
                "slot {slot}: tangent {:?}, want +X",
                frame.tangent
            );
            assert_eq!(
                frame.handedness, 1.0,
                "slot {slot}: identity UVs are right-handed"
            );
        }
    }

    /// Flipping v across the quad flips the bitangent, so the handedness
    /// sign flips while the tangent stays +X — the handedness encoding
    /// pinned in both directions. Derivation: dv2 = -1 makes
    /// `det = -1`, `T = (dv2·e1 − dv1·e2)/det = +X`, and
    /// `B = (du1·e2 − du2·e1)/det = −Y`, so `dot(cross(N, T), B) < 0`.
    #[test]
    fn v_flipped_uvs_flip_handedness() {
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let normals = [[0.0, 0.0, 1.0]; 6];
        let uvs = [
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
        ];
        let indices: Vec<u32> = (0..6).collect();
        let frames = vertex_tangent_frames(&positions, &normals, &uvs, &indices);
        for (slot, frame) in frames.iter().enumerate() {
            assert!(
                (frame.tangent[0] - 1.0).abs() < 1e-6,
                "slot {slot}: tangent {:?}, want +X (u is unchanged)",
                frame.tangent
            );
            assert_eq!(
                frame.handedness, -1.0,
                "slot {slot}: the v flip must flip the sign"
            );
        }
    }

    /// A degenerate UV triangle (all three uvs coincident) contributes
    /// nothing: its slots take the deterministic fallback frame — for
    /// the +Z normal that is `cross(+Z, +X) = +Y` — while the valid
    /// triangle's slots keep their real frame.
    #[test]
    fn degenerate_uv_triangle_contributes_nothing() {
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [5.0, 0.0, 0.0],
            [6.0, 0.0, 0.0],
            [5.0, 1.0, 0.0],
        ];
        let normals = [[0.0, 0.0, 1.0]; 6];
        let uvs = [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [9.0, 9.0],
            [9.0, 9.0],
            [9.0, 9.0],
        ];
        let indices: Vec<u32> = (0..6).collect();
        let frames = vertex_tangent_frames(&positions, &normals, &uvs, &indices);
        for (slot, frame) in frames.iter().enumerate().take(3) {
            assert!(
                (frame.tangent[0] - 1.0).abs() < 1e-6,
                "slot {slot}: the valid triangle's frame"
            );
        }
        for (slot, frame) in frames.iter().enumerate().skip(3) {
            assert!(
                (frame.tangent[1] - 1.0).abs() < 1e-6 && frame.tangent[0].abs() < 1e-6,
                "slot {slot}: the fallback for +Z is +Y, got {:?}",
                frame.tangent
            );
            assert_eq!(frame.handedness, 1.0);
        }
    }

    /// A UV seam splits weld groups: two triangles share a corner's
    /// POSITION but carry different uvs there, so the corners do NOT
    /// weld and each group takes only its own triangle's frame. The
    /// seam corner on the XY-plane side reads `+X`; the same position
    /// on the XZ-plane side (v runs along +Z) reads `+Z`.
    #[test]
    fn uv_seam_splits_weld_groups() {
        // Triangle A: the XY quad half (T = +X). Slot 3 duplicates
        // slot 0's POSITION but not its uv — the seam.
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
        ];
        let normals = [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let uvs = [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [2.0, 0.0],
            [3.0, 0.0],
            [3.0, 1.0],
        ];
        let indices: Vec<u32> = (0..6).collect();
        let frames = vertex_tangent_frames(&positions, &normals, &uvs, &indices);
        assert!(
            (frames[0].tangent[0] - 1.0).abs() < 1e-6 && frames[0].tangent[2].abs() < 1e-6,
            "the XY side of the seam: T = +X, got {:?}",
            frames[0].tangent
        );
        assert!(
            (frames[3].tangent[2] - 1.0).abs() < 1e-6 && frames[3].tangent[0].abs() < 1e-6,
            "the XZ side of the seam: T = +Z (u runs along +Z there), got {:?}",
            frames[3].tangent
        );
        assert_eq!(frames[3].handedness, 1.0);
    }

    /// Untrusted input never panics: out-of-range indices and a
    /// remainder entry are skipped; every valid slot still gets a
    /// frame (the fallback for untouched groups).
    #[test]
    fn out_of_range_and_remainder_indices_never_panic() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let normals = [[0.0, 0.0, 1.0]; 3];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        let indices = [0, 1, 999, 42];
        let frames = vertex_tangent_frames(&positions, &normals, &uvs, &indices);
        assert_eq!(
            frames.len(),
            3,
            "one frame per slot, whatever the indices say"
        );
        for frame in &frames {
            let len = dot(frame.tangent, frame.tangent).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-6,
                "every frame is unit, even the fallback"
            );
        }
    }

    /// A zero normal cannot define a perpendicular; the documented
    /// deterministic answer is `+X`, handedness +1.
    #[test]
    fn zero_normal_takes_the_degenerate_frame() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let normals = [[0.0, 0.0, 0.0]; 3];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        let indices = [0, 1, 2];
        let frames = vertex_tangent_frames(&positions, &normals, &uvs, &indices);
        for frame in &frames {
            assert_eq!(frame.tangent, [1.0, 0.0, 0.0]);
            assert_eq!(frame.handedness, 1.0);
        }
    }

    /// The frame invariants on a nontrivial mixed case: every tangent
    /// unit-length and perpendicular to its slot's normal.
    #[test]
    fn frames_are_unit_and_perpendicular() {
        // A triangle pair with different planes and uv scales.
        let positions = [
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 3.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 4.0, 5.0],
        ];
        let normals = [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        ];
        let uvs = [
            [0.0, 0.0],
            [0.5, 0.0],
            [0.5, 0.75],
            [0.0, 0.0],
            [0.0, 1.0],
            [1.25, 1.0],
        ];
        let indices: Vec<u32> = (0..6).collect();
        let frames = vertex_tangent_frames(&positions, &normals, &uvs, &indices);
        for (slot, frame) in frames.iter().enumerate() {
            let len = dot(frame.tangent, frame.tangent).sqrt();
            assert!((len - 1.0).abs() < 1e-6, "slot {slot}: unit tangent");
            assert!(
                dot(frame.tangent, normals[slot]).abs() < 1e-6,
                "slot {slot}: tangent perpendicular to the normal"
            );
            assert!(frame.handedness == 1.0 || frame.handedness == -1.0);
        }
    }
}
