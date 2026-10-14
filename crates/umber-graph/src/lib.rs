//! umber-graph — the original procedural node-graph engine.
//!
//! Wave 4 scope (docs/specs/requirements.md §4): DAG, topological eval,
//! dirty-region propagation; graphs serialize as .mtlx documents where
//! standard MaterialX nodes exist, with DECLARED CUSTOM NODEDEFS for
//! painter-specific nodes (flood fill, edge detect, mesh-map generators).
//! No automatic full-fidelity interchange is claimed (adversarial
//! review finding #2, docs/research/06).
//!
//! Wave 1 scope: the node vocabulary skeleton the workspace compiles on.
//! Wave 3 (this slice): the DAG core — [`topo`] with validation
//! (dangling refs, duplicate ids) and topological evaluation (Kahn's
//! algorithm, cycle rejection, inputs resolved before consumers run).
//! Wave 5 slice 1: the typed eval engine — [`value`] (Uniform/Image
//! value space, RGBA8 buffers) and [`eval`] (registry-dispatched
//! evaluation, dirty-aware cached re-evaluation).

pub mod anchor;
pub mod eval;
pub mod material;
pub mod mtlx;
pub mod mtlx_doc;
pub mod mtlx_import;
pub mod nodes;
pub mod topo;
pub mod value;

pub use anchor::AnchorRef;
pub use eval::{
    eval_graph, eval_graph_cached, EvalCache, EvalContext, EvalError, NodeImpl, NodeRegistry,
};
pub use topo::{Edge, Graph, GraphError};
pub use value::{decode_png_rgba8, encode_png_rgba8, ImageBuffer, ImageError, NodeOutput};

/// Builds the shared registry: the engine seed plus every merged node
/// family (generator, filter + color, mask + spatial, anchor, synth) plus
/// the dynamic-material nodes (`material_output`).
///
/// The Graph panel and the material eval driver share this one registry —
/// no drift between the surface a definition is authored on and the one it
/// evaluates on — and the panel can open a definition containing
/// `material_output`.
///
/// [`NodeRegistry::seeded`]: crate::NodeRegistry::seeded
pub fn full_registry() -> NodeRegistry {
    let mut registry = NodeRegistry::seeded();
    nodes::register_generator_nodes(&mut registry);
    nodes::register_filter_color_nodes(&mut registry);
    nodes::register_mask_spatial_nodes(&mut registry);
    anchor::register_anchor_nodes(&mut registry);
    nodes::register_synth_nodes(&mut registry);
    material::register_material_nodes(&mut registry);
    registry
}

/// A node in a graph. The Wave-4 engine evaluates these topologically.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: u64,
    /// MaterialX standard node name, or our custom nodedef name.
    pub node_def: String,
    pub params: Vec<(String, ParamValue)>,
    /// The node's graph-space position on the editor canvas (wave-6,
    /// `docs/specs/graph-canvas-design.md`). `None` = never placed: the
    /// canvas lays it out with its deterministic id-hash scatter.
    ///
    /// Editor-only data — evaluation never reads it, so moving a node
    /// dirties nothing. The crate has no serde, so the design's
    /// `#[serde(default)]` additive rule is enforced by the persistence
    /// carry instead: the app's Graph panel emits it as a `__canvas_pos`
    /// `vector2` input in its `.mtlx` string (Vec2 values round-trip
    /// exactly — see `mtlx::tests::round_trip_preserves_data`) and lifts
    /// it back out on load; documents without one load as `None`. The
    /// mtlx layer itself neither emits nor parses this field.
    pub canvas: Option<[f32; 2]>,
}

/// Parameter values, matching the MaterialX value space we target.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Color([f32; 3]),
    Int(i32),
    Bool(bool),
    /// Reference to another node's output.
    NodeRef(u64),
    /// Reference to an image asset (content-hash path).
    Asset(String),
    /// A layer-channel anchor (§3:42): any node input can bind one within
    /// the same texture set; the eval resolves it to the layer's
    /// composited channel at eval time (see [`anchor`]).
    Anchor(anchor::AnchorRef),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_vocabulary_compiles() {
        let n = Node {
            id: 1,
            node_def: "noise_fractal3d".into(),
            params: vec![
                ("scale".into(), ParamValue::Float(8.0)),
                ("octaves".into(), ParamValue::Int(5)),
            ],
            canvas: None,
        };
        assert_eq!(n.params.len(), 2);
    }

    #[test]
    fn full_registry_holds_every_family_including_material_output() {
        // The one shared registry (Graph panel + material driver): the seed
        // plus every register_* family plus the material nodes. Each
        // representative must dispatch — a name listed but unregistered
        // would fail a definition at eval time, not at authoring time.
        let registry = full_registry();
        for def in [
            "uniform",
            "passthrough",
            "noise_perlin",
            "mix",
            "flood_fill",
            "mesh_map_generator",
            "anchor",
            "synth",
            "material_output",
        ] {
            assert!(
                registry.get(def).is_some(),
                "full_registry must dispatch {def}"
            );
        }
        // And a material_output chain evaluates through it (the driver's
        // registry contract, headless).
        let mut g = Graph::new();
        g.add_node(Node {
            id: 1,
            node_def: "uniform".into(),
            params: vec![("color".into(), ParamValue::Color([0.0, 1.0, 0.0]))],
            canvas: None,
        });
        g.add_node(Node {
            id: 2,
            node_def: "material_output".into(),
            params: vec![("channel".into(), ParamValue::Asset("baseColor".into()))],
            canvas: None,
        });
        g.add_edge(Edge {
            from: 1,
            to: 2,
            input: "input".into(),
        });
        let out = eval_graph(
            &g,
            &registry,
            std::collections::HashMap::new(),
            &EvalContext::new(1, 1),
        )
        .expect("material chain evaluates on the shared registry");
        match &out[&2] {
            NodeOutput::Image(buf) => {
                assert_eq!(buf.data, vec![0, 255, 0, 255]);
            }
            other => panic!("expected an Image, got {other:?}"),
        }
    }
}
