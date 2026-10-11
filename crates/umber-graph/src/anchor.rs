//! Anchors (§3:42): typed `(layer, channel)` references a node-graph input
//! binds within the same texture set.
//!
//! The graph eval resolves anchors to the layer's composited channel at
//! eval time through [`EvalContext::external`]: the app bridge uploads each
//! bound layer channel under [`AnchorRef::key`] (`"anchor:{id}:{channel}"` —
//! the same key `umber-core`'s `AnchorRef` uses, so the bridge converts
//! without renaming), and [`resolve_anchor_inputs`] folds those externals
//! into the node's inputs. A bound anchor with no external — the layer was
//! deleted after binding, or never composited — is [`EvalError::Anchor`],
//! never a silent default.
//!
//! [`EvalContext::external`]: crate::EvalContext::external
//! [`EvalError::Anchor`]: crate::EvalError::Anchor
//!
//! # Judgment calls
//!
//! * This `AnchorRef` mirrors `umber-core`'s field-for-field instead of
//!   depending on it: `umber-graph` stays dependency-minimal (only
//!   `thiserror`), and the app bridge converts between the two.
//! * Anchors ride `ParamValue::Anchor` (per-input binding — any node input
//!   can bind one) AND the `anchor` node below (canvas wiring for users
//!   who want a visible node). Both resolve through the same externals.

use crate::{EvalContext, EvalError, NodeImpl, NodeOutput, ParamValue};
use std::collections::HashMap;
use std::sync::Arc;

/// A typed reference to one layer's channel within the same texture set.
///
/// Field-identical to `umber-core`'s `AnchorRef` (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchorRef {
    /// The referenced layer's id (same texture set as the binding graph).
    pub layer_id: u64,
    /// The channel name (e.g. `"baseColor"`, `"AO"`).
    pub channel: String,
}

impl AnchorRef {
    /// Binds `layer_id`'s `channel`.
    pub fn new(layer_id: u64, channel: impl Into<String>) -> Self {
        Self {
            layer_id,
            channel: channel.into(),
        }
    }

    /// The externals key the app bridge uploads the composited channel
    /// under: `"anchor:{layer_id}:{channel}"`.
    pub fn key(&self) -> String {
        format!("anchor:{}:{}", self.layer_id, self.channel)
    }
}

/// Resolves one node's `Anchor` params against `external` (the effective
/// context externals: `ctx.external` + per-eval inputs): each
/// `(param_name, Anchor)` becomes an `(input_name, NodeOutput)` entry so
/// any node consumes its bound layer channel as a regular named input —
/// no per-node anchor plumbing needed.
///
/// # Errors
///
/// [`EvalError::Anchor`] (constructed with `node: 0` — the dispatcher
/// stamps the id) when an anchor has no external: the layer was deleted
/// after binding, or the bridge never composited it.
pub fn resolve_anchor_inputs(
    node_id: u64,
    params: &[(String, ParamValue)],
    external: &HashMap<String, NodeOutput>,
) -> Result<Vec<(String, NodeOutput)>, EvalError> {
    let mut out = Vec::new();
    for (name, pv) in params {
        if let ParamValue::Anchor(anchor) = pv {
            let value = external
                .get(&anchor.key())
                .ok_or_else(|| EvalError::Anchor {
                    node: node_id,
                    layer: anchor.layer_id,
                    channel: anchor.channel.clone(),
                    reason: "layer deleted or never composited".into(),
                })?;
            out.push((name.clone(), value.clone()));
        }
    }
    Ok(out)
}

/// The `anchor` node: graph-side handle for a bound layer channel (the
/// canvas-visible form of a [`ParamValue::Anchor`] binding).
///
/// Params `layer_id: Int` (the layer; negative ids are `BadParam`) and
/// `channel: Asset` (the channel name string). Outputs the external under
/// [`AnchorRef::key`] EXACTLY (clone). A missing entry is
/// [`EvalError::Anchor`] naming the layer and channel.
pub struct AnchorNode;

impl NodeImpl for AnchorNode {
    fn eval(
        &self,
        _inputs: Vec<(String, NodeOutput)>,
        params: &[(String, ParamValue)],
        ctx: &EvalContext,
    ) -> Result<NodeOutput, EvalError> {
        let layer_id = match params.iter().find(|(n, _)| n == "layer_id") {
            Some((_, ParamValue::Int(id))) if *id >= 0 => *id as u64,
            _ => {
                return Err(EvalError::BadParam {
                    node: 0,
                    param: "layer_id".into(),
                });
            }
        };
        let channel = match params.iter().find(|(n, _)| n == "channel") {
            Some((_, ParamValue::Asset(s))) => s.clone(),
            _ => {
                return Err(EvalError::BadParam {
                    node: 0,
                    param: "channel".into(),
                });
            }
        };
        let anchor = AnchorRef::new(layer_id, channel);
        ctx.external
            .get(&anchor.key())
            .cloned()
            .ok_or_else(|| EvalError::Anchor {
                node: 0,
                layer: anchor.layer_id,
                channel: anchor.channel,
                reason: "layer deleted or never composited".into(),
            })
    }
}

/// Registers the `anchor` node on `registry`.
pub fn register_anchor_nodes(registry: &mut crate::NodeRegistry) {
    registry.register("anchor", Arc::new(AnchorNode));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{eval_graph, EvalContext, Graph, Node, NodeRegistry};
    use std::collections::HashMap;

    fn image_output() -> NodeOutput {
        NodeOutput::Image(crate::ImageBuffer::filled(2, 2, [9, 9, 9, 255]).expect("2x2 fills"))
    }

    #[test]
    fn anchor_key_is_stable() {
        assert_eq!(AnchorRef::new(7, "baseColor").key(), "anchor:7:baseColor");
    }

    #[test]
    fn anchor_param_resolves_to_the_layer_channel() {
        // passthrough(2) with an Anchor param bound on input "in": the
        // anchor value must arrive as the node's input.
        let mut g = Graph::new();
        g.add_node(Node {
            id: 2,
            node_def: "passthrough".into(),
            params: vec![(
                "in".into(),
                ParamValue::Anchor(AnchorRef::new(7, "baseColor")),
            )],
            canvas: None,
        });
        let mut inputs = HashMap::new();
        inputs.insert("anchor:7:baseColor".to_string(), image_output());
        let out = eval_graph(&g, &NodeRegistry::seeded(), inputs, &EvalContext::new(2, 2))
            .expect("anchored graph evaluates");
        assert_eq!(out[&2], image_output());
    }

    #[test]
    fn anchor_on_deleted_layer_is_a_typed_error() {
        // No external for the bound anchor: the layer was deleted (or was
        // never composited) — a typed Anchor error naming it, not a
        // silent default or a generic MissingInput.
        let mut g = Graph::new();
        g.add_node(Node {
            id: 2,
            node_def: "passthrough".into(),
            params: vec![(
                "in".into(),
                ParamValue::Anchor(AnchorRef::new(7, "baseColor")),
            )],
            canvas: None,
        });
        let err = eval_graph(
            &g,
            &NodeRegistry::seeded(),
            HashMap::new(),
            &EvalContext::new(2, 2),
        )
        .expect_err("deleted layer must fail");
        match err {
            EvalError::Anchor {
                node,
                layer,
                channel,
                ..
            } => {
                assert_eq!(node, 2, "dispatcher stamps the consumer id");
                assert_eq!(layer, 7);
                assert_eq!(channel, "baseColor");
            }
            other => panic!("expected Anchor, got {other:?}"),
        }
    }

    #[test]
    fn anchor_node_reads_external_or_reports_anchor() {
        let mut registry = NodeRegistry::new();
        register_anchor_nodes(&mut registry);
        let imp = registry.get("anchor").expect("registered");
        let mut ctx = EvalContext::new(2, 2);
        ctx.external.insert("anchor:3:AO".into(), image_output());
        let params = vec![
            ("layer_id".into(), ParamValue::Int(3)),
            ("channel".into(), ParamValue::Asset("AO".into())),
        ];
        assert_eq!(
            imp.eval(vec![], &params, &ctx).expect("resolves"),
            image_output()
        );
        let bare = EvalContext::new(2, 2);
        match imp.eval(vec![], &params, &bare) {
            Err(EvalError::Anchor { layer, channel, .. }) => {
                assert_eq!(layer, 3);
                assert_eq!(channel, "AO");
            }
            other => panic!("expected Anchor, got {other:?}"),
        }
        // Negative layer ids and mistyped channels are BadParam.
        let bad_id = vec![
            ("layer_id".into(), ParamValue::Int(-1)),
            ("channel".into(), ParamValue::Asset("AO".into())),
        ];
        assert!(matches!(
            imp.eval(vec![], &bad_id, &ctx),
            Err(EvalError::BadParam { .. })
        ));
    }
}
