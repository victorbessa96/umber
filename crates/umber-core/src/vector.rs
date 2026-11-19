//! The live vector-path data model (K1 of the path-tool vector wave,
//! `razul-vector-model`, 2026-11-18 — brief
//! `docs/loop/briefs/2026-11-18-umber-vector-model.md`).
//!
//! A vector layer (`LayerKind::Vector`) carries ONE [`VectorShape`] in
//! `Layer::vector`: bezier anchors with in/out handles in UV space, a fill
//! color, and an optional stroke. K1 is model-only: the shape composites
//! as fully transparent until K2's rasterizer lands, and there is no SVG
//! write side anywhere (K5 owns io).
//!
//! Serialization is additive (`#[serde(default)]` on the `Layer` field),
//! so projects saved before this row load with `vector == None`.

use serde::{Deserialize, Serialize};

/// One bezier anchor: position plus in/out control handles, all in UV
/// space (`0..1` over the active tile frame). A zero handle equals the
/// anchor position (a corner point).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub pos: [f32; 2],
    pub in_handle: [f32; 2],
    pub out_handle: [f32; 2],
}

impl Anchor {
    /// A corner anchor at `pos` (both handles collapsed onto the point).
    pub fn corner(pos: [f32; 2]) -> Self {
        Self {
            pos,
            in_handle: pos,
            out_handle: pos,
        }
    }
}

/// An optional stroke around the shape's outline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// Stroke width in UV units.
    pub width: f32,
    /// sRGBA color.
    pub color: [u8; 4],
}

/// The shape a vector layer holds. `anchors` in path order; `closed`
/// joins the last anchor back to the first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorShape {
    pub closed: bool,
    /// Fill color, sRGBA.
    pub fill: [u8; 4],
    pub stroke: Option<Stroke>,
    pub anchors: Vec<Anchor>,
}

impl VectorShape {
    /// A degenerate empty shape (no anchors): rasterizes to nothing.
    pub fn empty() -> Self {
        Self {
            closed: false,
            fill: [0, 0, 0, 0],
            stroke: None,
            anchors: Vec::new(),
        }
    }

    /// Whether the shape can cover any texel (K2's rasterizer gate).
    pub fn is_empty(&self) -> bool {
        self.anchors.len() < 2
    }
}

/// The vector layer's parameters, held in `Layer::vector` (`Some` exactly
/// for [`crate::layers::LayerKind::Vector`] layers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorParams {
    pub shape: VectorShape,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_corner_collapses_handles() {
        let a = Anchor::corner([0.25, 0.75]);
        assert_eq!(a.in_handle, a.pos);
        assert_eq!(a.out_handle, a.pos);
    }

    #[test]
    fn shape_round_trips_through_json_deterministically() {
        let shape = VectorShape {
            closed: true,
            fill: [200, 30, 40, 255],
            stroke: Some(Stroke {
                width: 0.01,
                color: [10, 10, 10, 255],
            }),
            anchors: vec![Anchor::corner([0.0, 0.0]), Anchor::corner([1.0, 1.0])],
        };
        let s = serde_json::to_string(&shape).expect("serialize");
        let back: VectorShape = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(shape, back);
        let s2 = serde_json::to_string(&shape).expect("serialize again");
        assert_eq!(s, s2, "serialization is deterministic");
    }

    #[test]
    fn empty_shape_reports_empty() {
        assert!(VectorShape::empty().is_empty());
        let one = VectorShape {
            anchors: vec![Anchor::corner([0.5, 0.5])],
            ..VectorShape::empty()
        };
        assert!(one.is_empty(), "one anchor cannot cover anything");
    }
}
