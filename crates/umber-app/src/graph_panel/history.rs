//! The procedural graph's undo journal (wave-procedural-history, kill 1).
//!
//! [`umber_core::undo::UndoStack`] is generic over any [`Command`] — its
//! own doc comment names "procedural-graph edits" as a future use; this
//! module is that use. Graph edits were the last non-undoable edit
//! class in the app: every layer edit rides `Document::run`, every
//! stroke the paint journal, but graph topology AND param edits rode
//! NOTHING until here (the roadmap row's "param edits ride
//! `Document::run`" premise was wrong — verified: no journal reference
//! anywhere in the panel before this kill).
//!
//! The panel is the command's `Doc`: topology edits touch the node
//! list, the edge list, the dirty set, `next_id`, `output_node` and
//! `selected`, and all of those live on [`GraphPanel`], so this child
//! module reaches them through `super` (privacy is module-tree scoped:
//! a descendant sees its parent's private fields).
//!
//! Capture-on-apply, the `LayerCommand` precedent: the panel's edit
//! methods build commands holding INTENT only; `apply` captures the
//! "before" state `revert` needs, so a command re-applied by redo
//! restores the exact same ids and edges (`AddNode` re-uses its
//! captured id on redo — a fresh id would break the exact-inverse
//! contract and strangle the journal's LIFO guarantees).
//!
//! Reverts only ever run through `UndoStack`'s LIFO ops (`push`,
//! `undo`, `redo`, `jump` — jump is sequential undo/redo), never
//! standalone: `SetParam`'s revert takes its captured old and relies
//! on the next apply to re-capture, so a caller reverting one command
//! twice without an intervening apply would mis-read the param-added
//! case. That shape is forbidden by this module's contract and must
//! not be introduced by the follow-up unified-timeline kill.
//!
//! Kill 1 is panel-local: the Graph panel's own Undo/Redo buttons
//! drive the journal. The unified timeline (Ctrl+Z, the History
//! panel's rows, click-to-time-travel) is the named follow-up kill —
//! it needs `main.rs` (the key dispatch) and `history_panel.rs`, both
//! outside this kill's surface.

use umber_core::undo::{Command, UndoStack};
use umber_graph::{Edge, Node, ParamValue};

use super::GraphPanel;

/// Journal capacity, the `Document` precedent's size. Graph commands
/// hold small scalars only, so the byte budget never binds.
pub(crate) const HISTORY_MAX: usize = 256;

/// One reversible graph edit. Variants hold intent; `apply` captures
/// the before-state for `revert` (the `LayerCommand` pattern).
pub(crate) enum GraphCommand {
    /// Adds a node, optionally placed at a canvas position. The
    /// allocated id is captured on first apply so redo restores the
    /// exact same node; the prior output is captured for the
    /// first-node adoption's revert.
    AddNode {
        def: String,
        pos: Option<[f32; 2]>,
        id: Option<u64>,
        prior_output: Option<u64>,
    },
    /// Removes a node and every edge touching it. `apply` captures the
    /// node, its edges, and the panel's prior output/selection (the
    /// two states the removal itself moves) so `revert` restores the
    /// whole configuration.
    RemoveNode {
        id: u64,
        removed: Option<(Node, Vec<Edge>)>,
        prior_output: Option<u64>,
        prior_selected: Option<u64>,
    },
    /// Wires `from` into `to`'s `input`, REPLACING any edge already
    /// feeding it (the canvas connect's semantics). The replaced
    /// edge's source is captured for `revert`.
    Connect {
        from: u64,
        to: u64,
        input: String,
        replaced_from: Option<u64>,
    },
    /// The list editor's raw insert (no replace): adds `from` into
    /// `to`'s `input` whatever already feeds it, preserving the
    /// pre-kill push behavior exactly.
    AddEdge { from: u64, to: u64, input: String },
    /// Removes the edge feeding `to`'s `input`; the old source is
    /// captured for `revert`.
    RemoveEdge {
        to: u64,
        input: String,
        old_from: Option<u64>,
    },
    /// Edits (or adds) one param on a node. The old value is captured
    /// on apply; a captured `None` means the param did not exist and
    /// `revert` removes it again.
    SetParam {
        node: u64,
        name: String,
        new: ParamValue,
        old: Option<ParamValue>,
    },
    /// Marks a node as the panel's output. The prior output is
    /// captured for `revert`.
    SetOutput { id: u64, old: Option<u64> },
}

impl Command for GraphCommand {
    type Doc = GraphPanel;

    fn apply(&mut self, panel: &mut GraphPanel) {
        match self {
            GraphCommand::AddNode {
                def,
                pos,
                id,
                prior_output,
            } => {
                // A redo re-applies with the id the FIRST apply captured
                // (the LayerCommand::Add precedent): the restored node
                // keeps its exact id. `next_id` never rewinds — ids
                // stay unique across the session even across undo/redo
                // cycles, so a journal entry can never point at a
                // re-minted id (a deliberate boundary, not a leak).
                let fresh = match *id {
                    Some(captured) => captured,
                    None => {
                        while panel.graph.nodes.iter().any(|n| n.id == panel.next_id) {
                            panel.next_id += 1;
                        }
                        let fresh = panel.next_id;
                        panel.next_id += 1;
                        fresh
                    }
                };
                panel.graph.add_node(Node {
                    id: fresh,
                    node_def: def.clone(),
                    params: super::default_params(def.as_str()),
                    canvas: *pos,
                });
                panel.dirty.insert(fresh);
                // Capture the pre-add output unconditionally: the
                // journal is LIFO, so at this command's undo no later
                // edit stands and the restore is always exact.
                *prior_output = panel.output_node;
                if panel.output_node.is_none() {
                    // The direct path's adoption rule: a graph with no
                    // output adopts the first node.
                    panel.output_node = Some(fresh);
                }
                *id = Some(fresh);
            }
            GraphCommand::RemoveNode {
                id,
                removed,
                prior_output,
                prior_selected,
            } => {
                let Some(node) = panel.graph.nodes.iter().find(|n| n.id == *id).cloned() else {
                    return;
                };
                let touching: Vec<Edge> = panel
                    .graph
                    .edges
                    .iter()
                    .filter(|e| e.from == *id || e.to == *id)
                    .cloned()
                    .collect();
                // Capture only what THIS removal moves (the direct
                // path's two reassignments); anything else stays
                // untouched by the revert.
                if panel.output_node == Some(*id) {
                    *prior_output = Some(*id);
                }
                if panel.selected == Some(*id) {
                    *prior_selected = Some(*id);
                }
                panel.graph.nodes.retain(|n| n.id != *id);
                panel.graph.edges.retain(|e| e.from != *id && e.to != *id);
                if panel.selected == Some(*id) {
                    panel.selected = None;
                }
                if panel.output_node == Some(*id) {
                    panel.output_node = panel.graph.nodes.iter().map(|n| n.id).max();
                }
                panel.mark_all_dirty();
                *removed = Some((node, touching));
            }
            GraphCommand::Connect {
                from,
                to,
                input,
                replaced_from,
            } => {
                debug_assert_ne!(from, to, "the dispatch refuses self-loops");
                *replaced_from = panel
                    .graph
                    .edges
                    .iter()
                    .find(|e| e.to == *to && e.input == *input)
                    .map(|e| e.from);
                panel
                    .graph
                    .edges
                    .retain(|e| !(e.to == *to && e.input == *input));
                panel.graph.edges.push(Edge {
                    from: *from,
                    to: *to,
                    input: input.clone(),
                });
                panel.dirty.insert(*to);
            }
            GraphCommand::AddEdge { from, to, input } => {
                panel.graph.edges.push(Edge {
                    from: *from,
                    to: *to,
                    input: input.clone(),
                });
                panel.dirty.insert(*to);
            }
            GraphCommand::RemoveEdge {
                to,
                input,
                old_from,
            } => {
                *old_from = panel
                    .graph
                    .edges
                    .iter()
                    .find(|e| e.to == *to && e.input == *input)
                    .map(|e| e.from);
                panel
                    .graph
                    .edges
                    .retain(|e| !(e.to == *to && e.input == *input));
                panel.dirty.insert(*to);
            }
            GraphCommand::SetParam {
                node,
                name,
                new,
                old,
            } => {
                let Some(n) = panel.graph.nodes.iter_mut().find(|n| n.id == *node) else {
                    return;
                };
                match n
                    .params
                    .iter_mut()
                    .find(|(k, _)| k.as_str() == name.as_str())
                {
                    Some((_, v)) => {
                        *old = Some(v.clone());
                        *v = new.clone();
                    }
                    None => n.params.push((name.clone(), new.clone())),
                }
                panel.dirty.insert(*node);
            }
            GraphCommand::SetOutput { id, old } => {
                *old = panel.output_node;
                panel.output_node = Some(*id);
            }
        }
    }

    fn revert(&mut self, panel: &mut GraphPanel) {
        match self {
            GraphCommand::AddNode {
                id: Some(id),
                prior_output,
                ..
            } => {
                panel.graph.nodes.retain(|n| n.id != *id);
                panel.graph.edges.retain(|e| e.from != *id && e.to != *id);
                // Restore the pre-add output unconditionally (None when
                // the add adopted; the untouched output otherwise).
                panel.output_node = *prior_output;
                panel.mark_all_dirty();
            }
            GraphCommand::RemoveNode {
                removed: Some((node, edges)),
                prior_output,
                prior_selected,
                ..
            } => {
                panel.graph.add_node(node.clone());
                for e in edges {
                    panel.graph.edges.push(e.clone());
                }
                if let Some(prior) = *prior_output {
                    panel.output_node = Some(prior);
                }
                if let Some(prior) = *prior_selected {
                    panel.selected = Some(prior);
                }
                panel.mark_all_dirty();
            }
            GraphCommand::Connect {
                to,
                input,
                replaced_from,
                ..
            } => {
                panel
                    .graph
                    .edges
                    .retain(|e| !(e.to == *to && e.input == *input));
                if let Some(prev) = *replaced_from {
                    panel.graph.edges.push(Edge {
                        from: prev,
                        to: *to,
                        input: input.clone(),
                    });
                }
                panel.dirty.insert(*to);
            }
            GraphCommand::AddEdge { from, to, input } => {
                panel
                    .graph
                    .edges
                    .retain(|e| !(e.from == *from && e.to == *to && e.input == *input));
                panel.dirty.insert(*to);
            }
            GraphCommand::RemoveEdge {
                to,
                input,
                old_from: Some(prev),
            } => {
                panel.graph.edges.push(Edge {
                    from: *prev,
                    to: *to,
                    input: input.clone(),
                });
                panel.dirty.insert(*to);
            }
            GraphCommand::SetParam {
                node, name, old, ..
            } => {
                let Some(n) = panel.graph.nodes.iter_mut().find(|n| n.id == *node) else {
                    return;
                };
                // `take`, not clone: the apply re-captures `old` on every
                // apply, so a redo rebuilds the captured state; taking
                // avoids holding the find's borrow across the retain
                // below (two &mut borrows of `params` would not
                // compile, and the take reads cleaner than a split).
                match old.take() {
                    Some(prev) => {
                        if let Some((_, v)) = n
                            .params
                            .iter_mut()
                            .find(|(k, _)| k.as_str() == name.as_str())
                        {
                            *v = prev;
                        }
                        // A captured old whose slot is missing cannot
                        // happen while the journal is LIFO (the node
                        // the apply saw still stands at this entry's
                        // undo); restoring a value into a missing slot
                        // would invent state, so a miss stays a miss.
                    }
                    None => {
                        // The apply ADDED the param (nothing to
                        // restore): remove it again.
                        n.params.retain(|(k, _)| k.as_str() != name.as_str());
                    }
                }
                panel.dirty.insert(*node);
            }
            GraphCommand::SetOutput { old, .. } => {
                panel.output_node = *old;
            }
            // A command that never applied (its capture fields are
            // still None) has nothing to revert.
            GraphCommand::AddNode { id: None, .. }
            | GraphCommand::RemoveNode { removed: None, .. }
            | GraphCommand::RemoveEdge { old_from: None, .. } => {}
        }
    }
}

impl GraphPanel {
    /// Runs one journaled graph command (apply + record for undo).
    /// The `mem::replace` dance splits the `self.history` / `self`
    /// double borrow: the journal lives on the panel, and the
    /// command's `Doc` is the panel itself.
    pub(crate) fn run_graph_command(&mut self, cmd: GraphCommand) {
        let mut history = std::mem::replace(&mut self.history, UndoStack::new(HISTORY_MAX));
        history.push(cmd, self);
        self.history = history;
    }

    /// One panel-local graph undo (the Graph panel's Undo button).
    /// The unified Ctrl+Z timeline is the named follow-up kill.
    pub fn undo_graph(&mut self) -> bool {
        let mut history = std::mem::replace(&mut self.history, UndoStack::new(HISTORY_MAX));
        let done = history.undo(self);
        self.history = history;
        done
    }

    /// One panel-local graph redo (the Graph panel's Redo button).
    pub fn redo_graph(&mut self) -> bool {
        let mut history = std::mem::replace(&mut self.history, UndoStack::new(HISTORY_MAX));
        let done = history.redo(self);
        self.history = history;
        done
    }

    /// Whether the graph journal holds an undo entry.
    pub fn can_undo_graph(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether the graph journal holds a redo entry.
    pub fn can_redo_graph(&self) -> bool {
        self.history.can_redo()
    }

    /// The Undo/Redo row (panel-local, kill 1 of
    /// wave-procedural-history). `add_enabled` keeps both buttons
    /// visible (discoverable) and disabled when the journal is empty.
    pub(crate) fn history_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.can_undo_graph(),
                    egui::Button::new(crate::i18n::tr("graph.undo")),
                )
                .clicked()
            {
                self.undo_graph();
            }
            if ui
                .add_enabled(
                    self.can_redo_graph(),
                    egui::Button::new(crate::i18n::tr("graph.redo")),
                )
                .clicked()
            {
                self.redo_graph();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::GraphPanel;
    use umber_graph::ParamValue;

    fn panel() -> GraphPanel {
        crate::i18n::install(crate::i18n::I18n::builtin());
        GraphPanel::new()
    }

    #[test]
    fn add_node_undo_redo_restores_the_same_id() {
        let mut p = panel();
        let id = p.add_node("noise_perlin");
        assert!(p.can_undo_graph());
        assert!(p.undo_graph());
        assert!(p.graph.nodes.is_empty(), "the add is undone");
        assert!(!p.can_undo_graph());
        assert!(p.can_redo_graph());
        assert!(p.redo_graph());
        let node = p.graph.nodes.first().expect("redo restored the node");
        assert_eq!(node.id, id, "redo restores the exact same id");
    }

    #[test]
    fn first_add_output_adoption_is_undone() {
        let mut p = panel();
        let id = p.add_node("passthrough");
        assert_eq!(p.output_node, Some(id), "the first node is adopted");
        assert!(p.undo_graph());
        assert_eq!(p.output_node, None, "the adoption is undone with the add");
        assert!(p.redo_graph());
        assert_eq!(p.output_node, Some(id), "redo re-adopts");
    }

    #[test]
    fn add_node_at_carries_its_position_through_undo() {
        let mut p = panel();
        let id = p.add_node_at("passthrough", [12.0, -3.5]);
        assert!(p.undo_graph());
        assert!(p.graph.nodes.is_empty());
        assert!(p.redo_graph());
        let node = p.graph.nodes.first().expect("redo restored the node");
        assert_eq!(node.id, id);
        assert_eq!(
            node.canvas,
            Some([12.0, -3.5]),
            "the placement rides the journal"
        );
    }

    #[test]
    fn connect_replacement_undo_restores_the_replaced_edge() {
        let mut p = panel();
        let a = p.add_node("noise_perlin");
        let b = p.add_node("passthrough");
        let c = p.add_node("passthrough");
        assert!(p.connect(a, b, "in"));
        assert!(p.connect(c, b, "in"), "the second connect replaces");
        assert_eq!(p.graph.edges.len(), 1);
        assert_eq!(p.graph.edges[0].from, c);
        assert!(p.undo_graph());
        assert_eq!(p.graph.edges.len(), 1);
        assert_eq!(
            p.graph.edges[0].from, a,
            "the replaced edge is restored by the undo"
        );
        assert!(p.redo_graph());
        assert_eq!(p.graph.edges[0].from, c);
    }

    #[test]
    fn add_edge_undo_removes_it_and_redo_restores() {
        let mut p = panel();
        let a = p.add_node("noise_perlin");
        let b = p.add_node("passthrough");
        assert!(p.add_edge(a, b, "in"));
        assert_eq!(p.graph.edges.len(), 1);
        assert!(p.undo_graph());
        assert!(p.graph.edges.is_empty(), "the raw insert is undone");
        assert!(p.redo_graph());
        assert_eq!(p.graph.edges.len(), 1, "redo restores the raw insert");
    }

    #[test]
    fn remove_node_undo_restores_node_edges_and_output() {
        let mut p = panel();
        let a = p.add_node("noise_perlin");
        let b = p.add_node("passthrough");
        assert!(p.connect(a, b, "in"));
        assert!(p.set_output_node(b));
        assert!(p.remove_node(b));
        assert!(p.graph.nodes.iter().all(|n| n.id != b));
        assert!(p.graph.edges.is_empty());
        assert_eq!(
            p.output_node,
            Some(a),
            "the removal reassigned the output to the max remaining id (the direct path's rule)"
        );
        assert!(p.undo_graph());
        assert_eq!(p.graph.nodes.len(), 2, "the node is back");
        assert_eq!(p.graph.edges.len(), 1, "its edge is back");
        assert_eq!(p.graph.edges[0].from, a);
        assert_eq!(p.graph.edges[0].to, b);
        assert_eq!(p.output_node, Some(b), "the prior output is restored");
    }

    #[test]
    fn remove_node_deselection_is_undone() {
        let mut p = panel();
        let id = p.add_node("passthrough");
        p.select(Some(id));
        assert!(p.remove_node(id));
        assert_eq!(p.selected, None);
        assert!(p.undo_graph());
        assert_eq!(p.selected, Some(id), "the removal's deselection is undone");
    }

    #[test]
    fn set_param_undo_restores_the_old_value() {
        let mut p = panel();
        let id = p.add_node("noise_perlin");
        assert!(p.set_param(id, "scale", ParamValue::Float(4.0)));
        assert!(p.set_param(id, "scale", ParamValue::Float(9.0)));
        assert!(p.undo_graph());
        let node = p.graph.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(
            node.params.iter().find(|(k, _)| k == "scale"),
            Some(&("scale".to_string(), ParamValue::Float(4.0))),
            "the pre-edit value is back"
        );
    }

    #[test]
    fn set_param_adding_a_param_is_undone_by_removing_it() {
        let mut p = panel();
        let id = p.add_node("passthrough");
        assert!(p.set_param(id, "brand-new", ParamValue::Float(1.0)));
        let node = p.graph.nodes.iter().find(|n| n.id == id).unwrap();
        assert!(node.params.iter().any(|(k, _)| k == "brand-new"));
        assert!(p.undo_graph());
        let node = p.graph.nodes.iter().find(|n| n.id == id).unwrap();
        assert!(
            node.params.iter().all(|(k, _)| k != "brand-new"),
            "the added param is removed again"
        );
        assert!(p.redo_graph());
        let node = p.graph.nodes.iter().find(|n| n.id == id).unwrap();
        assert!(node.params.iter().any(|(k, _)| k == "brand-new"));
    }

    #[test]
    fn set_output_undo_restores_the_prior_output() {
        let mut p = panel();
        let a = p.add_node("passthrough");
        let b = p.add_node("passthrough");
        assert_eq!(p.output_node, Some(a));
        assert!(p.set_output_node(b));
        assert!(p.undo_graph());
        assert_eq!(p.output_node, Some(a), "the prior output is restored");
    }

    #[test]
    fn a_new_edit_clears_the_redo_stack() {
        let mut p = panel();
        p.add_node("passthrough");
        assert!(p.undo_graph());
        assert!(p.can_redo_graph());
        p.add_node("noise_perlin");
        assert!(
            !p.can_redo_graph(),
            "a new edit invalidates the redo future, the UndoStack precedent"
        );
    }

    #[test]
    fn load_graph_clears_the_journal() {
        let mut p = panel();
        p.add_node("passthrough");
        assert!(p.can_undo_graph());
        p.load_graph(umber_graph::Graph::new());
        assert!(
            !p.can_undo_graph(),
            "a loaded graph is a state replacement; its journal dies with the old graph"
        );
        assert!(!p.can_redo_graph());
    }

    #[test]
    fn undo_button_reverts_the_last_graph_edit() {
        use egui_kittest::kittest::Queryable;
        let mut p = panel();
        let id = p.add_node("passthrough");
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 900.0))
            .build_ui_state(|ui, p: &mut GraphPanel| p.show(ui), p);
        harness.run();
        harness.get_by_label("Undo").click();
        harness.run();
        let p = harness.state();
        assert!(
            p.graph.nodes.iter().all(|n| n.id != id),
            "the button drove the journal"
        );
    }
}
