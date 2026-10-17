//! umber-bake — mesh-map bakers.
//!
//! Wave 3 scope (docs/specs/requirements.md §3): AO, normal (mesh +
//! high→low), curvature, position, thickness, ID, with cage controls,
//! dilation, and golden-image CI verification on lavapipe/WARP.
//! Compute passes share the umber-gpu tile pool.

pub mod ao;
pub mod curvature;
pub mod dilation;
pub mod id;
pub mod margin;
pub mod normal_map;
pub mod position;
pub mod skew;
pub mod thickness;
pub mod transfer;

pub use ao::{
    bake_ao_and_bent_mesh_with_occluder, bake_ao_mesh_with_occluder, bake_ao_tile,
    bake_tile_window, filter_mesh_for_tile, AoBakeError, AoBakeParams, AoWithBent, BakeTarget,
    PlaneDesc,
};
pub use curvature::{CurvatureBakeError, CurvatureParams};
pub use dilation::{DilateError, DilateParams};
pub use id::{IdBakeError, IdBakeParams, IdFlavor};
pub use normal_map::{TangentNormalBakeError, TangentNormalParams};
pub use skew::{
    affected_maps_for_param, apply_edge_protection, combine_skew_with_paint, skew_byte,
    skew_deg_from_byte, skew_deviation_deg, skew_map_from_deviations, skew_threshold_byte,
    AutoRebake, BakeParam, AUTOREBAKE_DEBOUNCE, DEFAULT_SKEW_THRESHOLD_DEG, EDGE_SAFE_NEUTRAL,
    MAX_SKEW_THRESHOLD_DEG, MIN_SKEW_THRESHOLD_DEG,
};
pub use thickness::{bake_thickness_mesh_with_occluder, ThicknessBakeError, ThicknessParams};
pub use transfer::{bake_transfer_mesh, TransferError, TransferMap, TransferParams};

/// Which triangles occlude a bake's rays
/// (`docs/specs/per-set-masked-bakes.md` §3, the occluder scope).
///
/// The reference tools agree on the default (the whole mesh occludes —
/// AO/thickness rays cross materials) and every one of them also
/// exposes a narrowing that Substance Painter cannot express per-set:
/// Marmoset's "Ignore Groups" is [`OccluderScope::OwnSet`], and the
/// glass-panel case ("everything BUT Glass darkens the gauges") is
/// [`OccluderScope::Sets`] — a set Painter cannot name.
///
/// Applies to AO, thickness and bent normals. Transfer's rays come from
/// the HIGH mesh, so its scope is the high-poly group, not this type.
///
/// v1 ship order (the design doc's): [`WholeMesh`] is today's behavior
/// byte for byte; the panel row lands with step 4.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OccluderScope {
    /// The whole mesh occludes — the default, matching today's output
    /// byte for byte (every ray sees every triangle).
    #[default]
    WholeMesh,
    /// Only the baked set's own triangles occlude (Marmoset's Ignore
    /// Groups). The raster submesh is untouched — only the RAYCAST
    /// mesh narrows.
    OwnSet,
    /// The named sets' triangles occlude, plus the baked set's own
    /// (a set never occludes itself out of its own bake's rays —
    /// `Sets(s)` always implies `s ∪ {the baked set}`; an empty list
    /// is therefore just the baked set, same as [`OwnSet`]).
    Sets(std::collections::BTreeSet<u16>),
}

impl OccluderScope {
    /// The occluding triangle indices, given the mesh's
    /// triangle→material map (`triangle_materials[i]` = material of
    /// triangle `i`; empty = single-material, so every scope resolves
    /// to every triangle). `baked_set` is the set being baked.
    ///
    /// The RASTER submesh is the caller's business (the baked set's own
    /// triangles through [`umber_mesh::filter_mesh_for_triangles`]);
    /// this answers only what the RAYS see.
    pub fn occluding_triangles(&self, triangle_materials: &[u16], baked_set: u16) -> Vec<u32> {
        let tris = |pred: &dyn Fn(usize) -> bool| {
            (0..triangle_materials.len() as u32)
                .filter(|&t| pred(t as usize))
                .collect::<Vec<u32>>()
        };
        match self {
            OccluderScope::WholeMesh => (0..triangle_materials.len() as u32).collect(),
            OccluderScope::OwnSet => tris(&|t| triangle_materials[t] == baked_set),
            OccluderScope::Sets(sets) => tris(&|t| {
                let m = triangle_materials[t];
                m == baked_set || sets.contains(&m)
            }),
        }
    }
}

/// The bake map types, named per Substance conventions for the
/// `TextureSetName_map` import naming (requirements §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BakeMap {
    AmbientOcclusion,
    Normal,
    Curvature,
    Position,
    Thickness,
    Id,
    Height,
    BentNormal,
    WorldSpaceNormal,
    /// The skew deviation channel (§6:57): per-texel raycast tangent
    /// deviation, paintable, consumed by the edge-protection guard
    /// (see [`skew`]).
    Skew,
}

impl BakeMap {
    /// The conventional on-disk suffix for imported mesh maps
    /// (`TextureSetName_ambient_occlusion.png` etc.).
    pub fn file_stem_suffix(self) -> &'static str {
        match self {
            BakeMap::AmbientOcclusion => "ambient_occlusion",
            BakeMap::Normal => "normal_base",
            BakeMap::Curvature => "curvature",
            BakeMap::Position => "position",
            BakeMap::Thickness => "thickness",
            BakeMap::Id => "id",
            BakeMap::Height => "height",
            BakeMap::BentNormal => "bent_normal",
            BakeMap::WorldSpaceNormal => "world_space_normal",
            BakeMap::Skew => "skew",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_naming_matches_substance_convention() {
        assert_eq!(
            BakeMap::AmbientOcclusion.file_stem_suffix(),
            "ambient_occlusion"
        );
        assert_eq!(
            BakeMap::WorldSpaceNormal.file_stem_suffix(),
            "world_space_normal"
        );
        assert_eq!(BakeMap::Skew.file_stem_suffix(), "skew");
    }

    #[test]
    fn occluder_scope_whole_mesh_is_every_triangle() {
        use super::OccluderScope;
        assert_eq!(
            OccluderScope::WholeMesh.occluding_triangles(&[0, 0, 1, 2], 1),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn occluder_scope_own_and_sets() {
        use super::OccluderScope;
        use std::collections::BTreeSet;
        let mats = [0u16, 0, 1, 2];
        assert_eq!(OccluderScope::OwnSet.occluding_triangles(&mats, 1), vec![2]);
        let mut sets = BTreeSet::new();
        sets.insert(1u16);
        // "Everything but Glass" occluding Body: Body's own + Hilt.
        assert_eq!(
            OccluderScope::Sets(sets).occluding_triangles(&mats, 0),
            vec![0, 1, 2]
        );
        // Empty Sets = the baked set only.
        assert_eq!(
            OccluderScope::Sets(BTreeSet::new()).occluding_triangles(&mats, 1),
            vec![2]
        );
        // Default.
        assert_eq!(OccluderScope::default(), OccluderScope::WholeMesh);
        // Empty mapping: nothing named; the caller falls back to whole mesh.
        assert_eq!(
            OccluderScope::OwnSet.occluding_triangles(&[], 0),
            Vec::<u32>::new()
        );
    }
}
