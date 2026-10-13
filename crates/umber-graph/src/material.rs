//! The `material_output` node: a dynamic material's per-channel declaration.
//!
//! (`docs/specs/dynamic-materials-design.md` §1.2.) A material's DECLARED
//! channel set is exactly the set of `material_output` nodes present — no
//! separate manifest, so definition and declaration can never drift:
//!
//! ```text
//! material_output { input: color3, channel: string }   // output: color3
//! ```
//!
//! * The port is named `input` (the existing convention — e.g.
//!   `direction_warp`'s `input`); the `channel` param is read as
//!   [`ParamValue::Asset`] (mtlx `string` parses to `Asset`, like the
//!   `anchor` node's `channel` — the shape this copies).
//! * The impl is a passthrough: its output is its `input` value UNCHANGED,
//!   [`NodeOutput::Image`] or [`NodeOutput::Uniform`] alike — the impl never
//!   errs on the variant, so a `Uniform` cannot fail the whole graph. (A
//!   `Uniform` arriving here is the driver's per-channel check: that
//!   channel publishes nothing while its siblings still publish.)
//! * Set-membership validation (does the set name this channel? exactly one
//!   output per channel?) is the eval driver's job, not the node's: the
//!   node sees params, never the texture set.
//!
//! [`ParamValue::Asset`]: crate::ParamValue::Asset
//! [`NodeOutput::Image`]: crate::NodeOutput::Image
//! [`NodeOutput::Uniform`]: crate::NodeOutput::Uniform

use crate::{EvalError, NodeImpl, NodeOutput, ParamValue};
use std::sync::Arc;

/// The `material_output` node: echoes its single input unchanged (the
/// canvas-visible declaration of one written channel).
///
/// Param `channel: Asset` (the channel name string, e.g. `"baseColor"`).
/// A missing or mistyped `channel` is [`EvalError::BadParam`]; zero inputs
/// is [`EvalError::MissingInput`]; more than one input is
/// [`EvalError::BadParam`] (an output declares exactly one value — the
/// `passthrough` arity precedent).
pub struct MaterialOutputNode;

impl NodeImpl for MaterialOutputNode {
    fn eval(
        &self,
        inputs: Vec<(String, NodeOutput)>,
        params: &[(String, ParamValue)],
        _ctx: &crate::EvalContext,
    ) -> Result<NodeOutput, EvalError> {
        match params.iter().find(|(n, _)| n == "channel") {
            Some((_, ParamValue::Asset(_))) => {}
            _ => {
                return Err(EvalError::BadParam {
                    node: 0,
                    param: "channel".into(),
                });
            }
        }
        // Exactly one input echoes unchanged (Image or Uniform alike);
        // anything else is a typed error — the `passthrough` arity shape,
        // written without an unreachable fallback value.
        let input_count = inputs.len();
        let mut iter = inputs.into_iter();
        match (iter.next(), iter.next()) {
            (Some((_, value)), None) => Ok(value),
            (None, _) => Err(EvalError::MissingInput {
                node: 0,
                input: "input".into(),
            }),
            _ => Err(EvalError::BadParam {
                node: 0,
                param: format!("expected exactly 1 input, got {input_count}"),
            }),
        }
    }
}

/// Registers the `material_output` node on `registry`.
pub fn register_material_nodes(registry: &mut crate::NodeRegistry) {
    registry.register("material_output", Arc::new(MaterialOutputNode));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{eval_graph, EvalContext, Graph, Node, NodeRegistry};
    use std::collections::HashMap;

    fn output_node(id: u64, channel: ParamValue) -> Node {
        Node {
            id,
            node_def: "material_output".into(),
            params: vec![("channel".into(), channel)],
            canvas: None,
        }
    }

    #[test]
    fn material_output_echoes_images_unchanged() {
        // uniform(red image) → material_output: the output is byte-identical.
        let mut g = Graph::new();
        g.add_node(Node {
            id: 1,
            node_def: "uniform".into(),
            params: vec![("color".into(), ParamValue::Color([1.0, 0.0, 0.0]))],
            canvas: None,
        });
        g.add_node(output_node(2, ParamValue::Asset("baseColor".into())));
        g.add_edge(crate::Edge {
            from: 1,
            to: 2,
            input: "input".into(),
        });
        let mut registry = NodeRegistry::seeded();
        register_material_nodes(&mut registry);
        let out = eval_graph(&g, &registry, HashMap::new(), &EvalContext::new(2, 2))
            .expect("material chain evaluates");
        assert_eq!(out[&2], out[&1], "the output echoes its input exactly");
        match &out[&2] {
            NodeOutput::Image(buf) => {
                assert_eq!((buf.width, buf.height), (2, 2));
                assert!(buf.data.chunks_exact(4).all(|px| px == [255, 0, 0, 255]));
            }
            other => panic!("expected an Image, got {other:?}"),
        }
    }

    #[test]
    fn material_output_echoes_uniforms_without_failing() {
        // A Uniform arriving here must NOT fail the graph: the driver turns
        // it into a per-channel skip while siblings still publish.
        let mut g = Graph::new();
        g.add_node(Node {
            id: 1,
            node_def: "uniform".into(),
            params: vec![("value".into(), ParamValue::Float(0.5))],
            canvas: None,
        });
        g.add_node(output_node(2, ParamValue::Asset("roughness".into())));
        g.add_edge(crate::Edge {
            from: 1,
            to: 2,
            input: "input".into(),
        });
        let mut registry = NodeRegistry::seeded();
        register_material_nodes(&mut registry);
        let out = eval_graph(&g, &registry, HashMap::new(), &EvalContext::new(2, 2))
            .expect("a Uniform input must not fail the graph");
        assert_eq!(
            out[&2],
            NodeOutput::Uniform(ParamValue::Float(0.5)),
            "the Uniform passes through untouched"
        );
    }

    #[test]
    fn material_output_rejects_a_missing_or_mistyped_channel() {
        // No `channel` param, or a non-Asset one, is BadParam — never a
        // silent default channel (that would paint the wrong channel).
        let mut registry = NodeRegistry::seeded();
        register_material_nodes(&mut registry);
        let imp = registry.get("material_output").expect("registered");
        let ctx = EvalContext::new(1, 1);
        let input = vec![(
            "input".to_string(),
            NodeOutput::Uniform(ParamValue::Float(1.0)),
        )];
        assert!(matches!(
            imp.eval(input.clone(), &[], &ctx),
            Err(EvalError::BadParam { node: 0, .. })
        ));
        let bad = vec![("channel".into(), ParamValue::Float(1.0))];
        assert!(matches!(
            imp.eval(input.clone(), &bad, &ctx),
            Err(EvalError::BadParam { node: 0, .. })
        ));
        // Zero inputs is MissingInput; two inputs is BadParam (the
        // passthrough arity precedent).
        let good = vec![("channel".into(), ParamValue::Asset("baseColor".into()))];
        assert!(matches!(
            imp.eval(Vec::new(), &good, &ctx),
            Err(EvalError::MissingInput { node: 0, .. })
        ));
        let two = vec![
            ("a".to_string(), NodeOutput::Uniform(ParamValue::Float(1.0))),
            ("b".to_string(), NodeOutput::Uniform(ParamValue::Float(2.0))),
        ];
        assert!(matches!(
            imp.eval(two, &good, &ctx),
            Err(EvalError::BadParam { node: 0, .. })
        ));
    }
}
