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
pub mod normal_map;
pub mod position;
pub mod skew;
pub mod thickness;
pub mod transfer;

pub use ao::{
    bake_ao_tile, bake_tile_window, filter_mesh_for_tile, AoBakeError, AoBakeParams, AoWithBent,
    BakeTarget, PlaneDesc,
};
pub use curvature::{CurvatureBakeError, CurvatureParams};
pub use dilation::{DilateError, DilateParams};
pub use id::{IdBakeError, IdBakeParams, IdFlavor};
pub use normal_map::{TangentNormalBakeError, TangentNormalParams};
pub use skew::{
    affected_maps_for_param, apply_edge_protection, combine_skew_with_paint,
    skew_byte, skew_deg_from_byte, skew_deviation_deg, skew_map_from_deviations,
    skew_threshold_byte, AutoRebake, BakeParam, AUTOREBAKE_DEBOUNCE,
    DEFAULT_SKEW_THRESHOLD_DEG, EDGE_SAFE_NEUTRAL, MAX_SKEW_THRESHOLD_DEG,
    MIN_SKEW_THRESHOLD_DEG,
};
pub use thickness::{ThicknessBakeError, ThicknessParams};
pub use transfer::{bake_transfer_mesh, TransferError, TransferMap, TransferParams};

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
}
