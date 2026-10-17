//! The per-frame composite snapshot builder (per-layer pool §4.2): the
//! document stack → the bottom-to-top rows the composite flush and the
//! anchor prefix fold. Pure over a [`LayerStack`](umber_core::layers::LayerStack)
//! plus the material refresh states, so every front end builds the SAME
//! rows: the shell each frame, the `umber-cli agent` session after each
//! paint step (it passes empty [`MaterialLayerStates`](crate::material_layer::MaterialLayerStates)
//! — it evaluates no materials, so their rows contribute nothing, which is
//! the truth of that session).
//!
//! Moved from umber-app's `main.rs` (2026-10-14) unchanged.

/// Builds the per-frame composite snapshot (§4.2) from the document
/// stack plus the Base row's visibility: bottom-to-top rows (index 0 is
/// the stack bottom, matching `LayerStack` and the compositor), Base
/// first, and the displayed `(tile, channel)`s (the active tile's
/// baseColor + height — plus the UV canvas's bound channel when the
/// caller passes it).
///
/// Material rows expand per contributing channel (§5.2): the shell reads
/// each layer's [`MaterialLayerStates`](crate::material_layer::MaterialLayerStates) plans and forms
/// `layer.opacity × channel.opacity` here, so opacity edits propagate
/// without a re-eval. The layer's own `blend_mode` stays inert on
/// materials; folders carry no planes (no entry); instances emit a row
/// over their SOURCE's planes (stage 5, see the Instance arm).
pub fn build_composite_snapshot(
    stack: &umber_core::layers::LayerStack,
    states: &crate::material_layer::MaterialLayerStates,
    base_visible: bool,
    displayed: Vec<(u16, String)>,
) -> crate::paint_state::CompositeSnapshot {
    use crate::paint_state::{
        PlaneOwner, SnapshotLayer, SnapshotLayerKind, SnapshotMaterialChannel,
    };
    let mut layers = vec![SnapshotLayer {
        owner: PlaneOwner::Base,
        layer_id: None,
        kind: SnapshotLayerKind::Paint,
        blend: umber_core::BlendMode::Normal,
        opacity: 1.0,
        visible: base_visible,
        mask_enabled: false,
        material_channels: None,
    }];
    for layer in &stack.layers {
        let owner = PlaneOwner::Layer(layer.id);
        let mask_enabled = layer.mask.as_ref().is_some_and(|mask| mask.gates());
        match layer.kind {
            umber_core::LayerKind::Material => {
                let channels: Vec<SnapshotMaterialChannel> = states
                    .state(layer.id)
                    .map(|state| {
                        state
                            .composite_channels()
                            .iter()
                            .map(|entry| SnapshotMaterialChannel {
                                channel: entry.channel.clone(),
                                blend: entry.blend,
                                opacity: layer.opacity * entry.opacity,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                layers.push(SnapshotLayer {
                    owner,
                    layer_id: Some(layer.id),
                    kind: SnapshotLayerKind::Material,
                    blend: layer.blend_mode,
                    opacity: layer.opacity,
                    visible: layer.visible,
                    mask_enabled,
                    material_channels: Some(channels),
                });
            }
            umber_core::LayerKind::Fill => {
                layers.push(SnapshotLayer {
                    owner,
                    layer_id: Some(layer.id),
                    kind: SnapshotLayerKind::Fill,
                    blend: layer.blend_mode,
                    opacity: layer.opacity,
                    visible: layer.visible,
                    mask_enabled,
                    material_channels: None,
                });
            }
            umber_core::LayerKind::Paint => {
                layers.push(SnapshotLayer {
                    owner,
                    layer_id: Some(layer.id),
                    kind: SnapshotLayerKind::Paint,
                    blend: layer.blend_mode,
                    opacity: layer.opacity,
                    visible: layer.visible,
                    mask_enabled,
                    material_channels: None,
                });
            }
            umber_core::LayerKind::Instance { .. } => {
                // Stage 5 (§5.3): an instance renders its SOURCE's planes
                // (resolved through instance chains) with its OWN
                // visibility, opacity and mask. A Paint/Fill source takes
                // the instance's own blend; a Material source keeps its
                // per-channel blends with the instance's opacity folded in
                // (the material row's `layer.opacity × entry.opacity`
                // shape). A dangling or Folder source contributes nothing.
                let Some(source) = resolve_instance_source(stack, layer.id) else {
                    continue;
                };
                let source_owner = PlaneOwner::Layer(source.id);
                match source.kind {
                    umber_core::LayerKind::Material => {
                        let channels: Vec<SnapshotMaterialChannel> = states
                            .state(source.id)
                            .map(|state| {
                                state
                                    .composite_channels()
                                    .iter()
                                    .map(|entry| SnapshotMaterialChannel {
                                        channel: entry.channel.clone(),
                                        blend: entry.blend,
                                        opacity: layer.opacity * entry.opacity,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        layers.push(SnapshotLayer {
                            owner: source_owner,
                            layer_id: Some(layer.id),
                            kind: SnapshotLayerKind::Material,
                            blend: layer.blend_mode,
                            opacity: layer.opacity,
                            visible: layer.visible,
                            mask_enabled,
                            material_channels: Some(channels),
                        });
                    }
                    umber_core::LayerKind::Paint | umber_core::LayerKind::Fill => {
                        let kind = if source.kind == umber_core::LayerKind::Fill {
                            SnapshotLayerKind::Fill
                        } else {
                            SnapshotLayerKind::Paint
                        };
                        layers.push(SnapshotLayer {
                            owner: source_owner,
                            layer_id: Some(layer.id),
                            kind,
                            blend: layer.blend_mode,
                            opacity: layer.opacity,
                            visible: layer.visible,
                            mask_enabled,
                            material_channels: None,
                        });
                    }
                    umber_core::LayerKind::Folder { .. }
                    | umber_core::LayerKind::Instance { .. } => {}
                }
            }
            umber_core::LayerKind::Folder { .. } => {
                // Folders carry no pixels (compositor judgment 4): no
                // entry (a missing plane is the transparent no-op).
            }
        }
    }
    crate::paint_state::CompositeSnapshot {
        layers,
        base_visible,
        displayed,
        set_layers: Default::default(),
    }
}

/// Follows `id`'s instance chain to the first non-instance layer (§5.3:
/// instances render their source transitively). `None` when the chain
/// dangles or loops — `set_instance_source` refuses cycles at attach
/// time, but a corrupt load must not hang the frame, so the walk is
/// bounded by the stack's length.
pub fn resolve_instance_source(
    stack: &umber_core::layers::LayerStack,
    id: u64,
) -> Option<&umber_core::layers::Layer> {
    let mut current = stack.layer(id)?;
    for _ in 0..=stack.layers.len() {
        match current.kind {
            umber_core::LayerKind::Instance { source_id } => {
                current = stack.layer(source_id)?;
            }
            _ => return Some(current),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint_state::{PlaneOwner, SnapshotLayerKind};
    use umber_core::layers::{LayerKind, LayerStack};

    #[test]
    fn base_is_first_and_carries_base_visible() {
        let stack = LayerStack::new();
        let snap = build_composite_snapshot(&stack, &Default::default(), false, Vec::new());
        assert_eq!(snap.layers.len(), 1, "an empty stack is still the Base row");
        assert_eq!(snap.layers[0].owner, PlaneOwner::Base);
        assert!(!snap.layers[0].visible, "base_visible=false hides Base");
        assert!(!snap.base_visible);
        let shown = build_composite_snapshot(&stack, &Default::default(), true, Vec::new());
        assert!(shown.layers[0].visible);
        assert!(shown.base_visible);
    }

    #[test]
    fn paint_and_fill_rows_carry_their_own_settings() {
        let mut stack = LayerStack::new();
        let paint = stack.add_layer("P", LayerKind::Paint);
        let fill = stack.add_layer("F", LayerKind::Fill);
        // stack.add_layer pushes BOTTOM-TO-TOP: the snapshot rows follow.
        let snap = build_composite_snapshot(&stack, &Default::default(), true, Vec::new());
        assert_eq!(snap.layers.len(), 3, "Base + paint + fill");
        assert_eq!(snap.layers[1].owner, PlaneOwner::Layer(paint));
        assert_eq!(snap.layers[1].kind, SnapshotLayerKind::Paint);
        assert_eq!(snap.layers[2].owner, PlaneOwner::Layer(fill));
        assert_eq!(snap.layers[2].kind, SnapshotLayerKind::Fill);
        // Defaults: visible, full opacity, no mask gate.
        for row in &snap.layers[1..] {
            assert!(row.visible);
            assert_eq!(row.opacity, 1.0);
            assert!(!row.mask_enabled);
        }
    }

    #[test]
    fn folders_contribute_no_row() {
        let mut stack = LayerStack::new();
        stack.add_layer("group", LayerKind::Folder { passthrough: false });
        let inside = stack.add_layer("in", LayerKind::Paint);
        let snap = build_composite_snapshot(&stack, &Default::default(), true, Vec::new());
        // Base + the paint INSIDE the folder; the folder itself has no row.
        assert_eq!(snap.layers.len(), 2);
        assert_eq!(snap.layers[1].owner, PlaneOwner::Layer(inside));
    }

    #[test]
    fn a_material_without_state_yields_no_channels_the_agent_path() {
        // umber-cli's agent session passes empty MaterialLayerStates —
        // its materials evaluate nothing, so the row carries empty
        // channels (the documented truth of that session).
        let mut stack = LayerStack::new();
        stack.add_layer("M", LayerKind::Material);
        let snap = build_composite_snapshot(&stack, &Default::default(), true, Vec::new());
        let mat = &snap.layers[1];
        assert_eq!(mat.kind, SnapshotLayerKind::Material);
        assert!(mat.material_channels.as_deref().unwrap().is_empty());
    }

    #[test]
    fn an_instance_renders_its_source_and_a_dangling_one_is_skipped() {
        let mut stack = LayerStack::new();
        let src = stack.add_layer("src", LayerKind::Paint);
        let inst = stack.add_layer("inst", LayerKind::Instance { source_id: src });
        let snap = build_composite_snapshot(&stack, &Default::default(), true, Vec::new());
        // Row 1 = the source's OWN paint row; row 2 = the instance
        // rendering the source's owner under the instance's id.
        assert_eq!(snap.layers.len(), 3, "Base + source + instance");
        assert_eq!(snap.layers[1].owner, PlaneOwner::Layer(src));
        assert_eq!(snap.layers[1].layer_id, Some(src));
        assert_eq!(snap.layers[2].owner, PlaneOwner::Layer(src));
        assert_eq!(snap.layers[2].layer_id, Some(inst), "stage 5's shape");
        // A dangling instance: source_id points at nothing.
        let gone = stack.add_layer("dangle", LayerKind::Instance { source_id: 9999 });
        let snap2 = build_composite_snapshot(&stack, &Default::default(), true, Vec::new());
        assert!(
            snap2.layers.iter().all(|l| l.layer_id != Some(gone)),
            "the dangling instance contributes no row"
        );
    }

    #[test]
    fn displayed_rides_through_untouched() {
        let stack = LayerStack::new();
        let displayed = vec![(1001u16, "baseColor".into()), (1002u16, "height".into())];
        let snap = build_composite_snapshot(&stack, &Default::default(), true, displayed.clone());
        assert_eq!(snap.displayed, displayed);
    }

    #[test]
    fn resolve_instance_source_walks_chains_and_stops() {
        let mut stack = LayerStack::new();
        let src = stack.add_layer("src", LayerKind::Paint);
        let mid = stack.add_layer("mid", LayerKind::Instance { source_id: src });
        let top = stack.add_layer("top", LayerKind::Instance { source_id: mid });
        let resolved = resolve_instance_source(&stack, top);
        assert_eq!(
            resolved.map(|l| l.id),
            Some(src),
            "the chain walks to the source"
        );
        // A dangling id resolves to None, not a hang.
        assert!(resolve_instance_source(&stack, 9999).is_none());
    }
}
