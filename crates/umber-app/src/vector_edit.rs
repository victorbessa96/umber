//! Document-backed vector-layer editing (K3). The active vector layer adopts
//! into a session working copy. Discrete edits write one `SetVector` command;
//! a drag previews that copy and writes one command on release, never per frame.

use umber_core::vector::{Anchor, VectorParams, VectorShape, MAX_VECTOR_ANCHORS};
use umber_core::LayerCommand;

use crate::document::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorHandle {
    Anchor,
    In,
    Out,
}

#[derive(Debug, Clone)]
struct Gesture {
    index: usize,
    handle: VectorHandle,
    before: VectorParams,
    document: Option<VectorParams>,
}

#[derive(Debug, Clone)]
pub struct VectorEditTool {
    target: Option<u64>,
    working: VectorShape,
    /// Document state corresponding to `working` when no gesture is active.
    /// `None` is a fresh vector layer whose first edit creates the params.
    adopted: Option<VectorParams>,
    selected: Option<(usize, VectorHandle)>,
    gesture: Option<Gesture>,
}

impl Default for VectorEditTool {
    fn default() -> Self {
        Self {
            target: None,
            working: VectorShape::empty(),
            adopted: None,
            selected: None,
            gesture: None,
        }
    }
}

impl VectorEditTool {
    pub fn target(&self) -> Option<u64> {
        self.target
    }

    pub fn is_editing(&self) -> bool {
        self.target.is_some()
    }

    pub fn has_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    pub fn shape(&self) -> &VectorShape {
        &self.working
    }

    pub fn selected(&self) -> Option<(usize, VectorHandle)> {
        self.selected
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Adopts the active layer without erasing an active gesture. Outside a
    /// gesture the document always wins, including undo/redo and external edits.
    /// `Some((id, None))` is a fresh vector layer with no params yet; it adopts
    /// an empty working shape and stays armed for its first anchor.
    pub fn adopt(&mut self, active: Option<(u64, Option<&VectorParams>)>) {
        let Some((id, params)) = active else {
            self.clear();
            return;
        };
        if self.target != Some(id) {
            self.target = Some(id);
            self.selected = None;
            self.gesture = None;
        }
        if self.gesture.is_none() {
            self.adopted = params.cloned();
            self.working = params
                .map(|params| params.shape.clone())
                .unwrap_or_else(|| VectorParams::default().shape);
            self.selected = self
                .selected
                .filter(|(index, _)| *index < self.working.anchors.len());
        }
    }

    pub fn select(&mut self, index: usize, handle: VectorHandle) -> bool {
        if index >= self.working.anchors.len() {
            return false;
        }
        self.selected = Some((index, handle));
        true
    }

    pub fn append(&mut self, doc: &mut Document, uv: [f32; 2]) -> bool {
        if self.target.is_none() || self.gesture.is_some() || !finite_uv(uv) {
            return false;
        }
        if self.working.anchors.len() >= MAX_VECTOR_ANCHORS {
            return false;
        }
        let before = self.params();
        self.working.anchors.push(Anchor::corner(uv));
        self.selected = Some((self.working.anchors.len() - 1, VectorHandle::Anchor));
        self.commit(doc, before)
    }

    pub fn insert_near_segment(&mut self, doc: &mut Document, uv: [f32; 2], tol: f32) -> bool {
        if self.target.is_none()
            || self.gesture.is_some()
            || self.working.anchors.len() < 2
            || self.working.anchors.len() >= MAX_VECTOR_ANCHORS
            || !finite_uv(uv)
            || !tol.is_finite()
            || tol < 0.0
        {
            return false;
        }
        let Some((segment, distance)) = nearest_segment(&self.working, uv) else {
            return false;
        };
        if distance > tol * tol {
            return false;
        }
        let at = if self.working.closed && segment == self.working.anchors.len() - 1 {
            self.working.anchors.len()
        } else {
            segment + 1
        };
        let before = self.params();
        self.working.anchors.insert(at, Anchor::corner(uv));
        self.selected = Some((at, VectorHandle::Anchor));
        self.commit(doc, before)
    }

    pub fn delete_near(&mut self, doc: &mut Document, uv: [f32; 2], tol: f32) -> bool {
        if !tol.is_finite() || tol < 0.0 {
            return false;
        }
        let Some(index) = nearest_anchor(&self.working, uv, tol) else {
            return false;
        };
        self.delete_anchor(doc, index)
    }

    /// Deletes exactly one visible anchor. UV right-clicks route here from
    /// every handle, including coincident in/out handles that would otherwise
    /// swallow the background response's hit.
    pub fn delete_anchor(&mut self, doc: &mut Document, index: usize) -> bool {
        if self.target.is_none() || self.gesture.is_some() || index >= self.working.anchors.len() {
            return false;
        }
        let before = self.params();
        self.working.anchors.remove(index);
        self.selected = match self.selected {
            Some((selected, _)) if selected == index => None,
            Some((selected, handle)) if selected > index => Some((selected - 1, handle)),
            selected => selected.filter(|(selected, _)| *selected < self.working.anchors.len()),
        };
        self.commit(doc, before)
    }

    pub fn set_closed(&mut self, doc: &mut Document, closed: bool) -> bool {
        if self.target.is_none() || self.gesture.is_some() || self.working.closed == closed {
            return false;
        }
        let before = self.params();
        self.working.closed = closed;
        self.commit(doc, before)
    }

    pub fn begin_drag(&mut self, index: usize, handle: VectorHandle) -> bool {
        if self.target.is_none() || self.gesture.is_some() || index >= self.working.anchors.len() {
            return false;
        }
        self.selected = Some((index, handle));
        self.gesture = Some(Gesture {
            index,
            handle,
            before: self.params(),
            document: self.adopted.clone(),
        });
        true
    }

    pub fn drag_to(&mut self, uv: [f32; 2]) -> bool {
        if !finite_uv(uv) {
            return false;
        }
        let Some(gesture) = self.gesture.as_ref() else {
            return false;
        };
        let Some(anchor) = self.working.anchors.get_mut(gesture.index) else {
            return false;
        };
        match gesture.handle {
            VectorHandle::Anchor => {
                let delta = [uv[0] - anchor.pos[0], uv[1] - anchor.pos[1]];
                anchor.pos = uv;
                anchor.in_handle = [
                    anchor.in_handle[0] + delta[0],
                    anchor.in_handle[1] + delta[1],
                ];
                anchor.out_handle = [
                    anchor.out_handle[0] + delta[0],
                    anchor.out_handle[1] + delta[1],
                ];
            }
            VectorHandle::In => anchor.in_handle = uv,
            VectorHandle::Out => anchor.out_handle = uv,
        }
        true
    }

    /// Completes one drag. `true` means the gesture closed, including a no-op;
    /// `false` means an external document edit invalidated and cancelled it.
    pub fn finish_drag(&mut self, doc: &mut Document) -> bool {
        let Some(gesture) = self.gesture.take() else {
            return false;
        };
        let Some(id) = self.target else {
            return false;
        };
        if !self.doc_matches(id, &gesture.document, doc) {
            return false;
        }
        if self.working.same_cache_key(&gesture.before.shape) {
            return true;
        }
        doc.run(LayerCommand::set_vector(id, self.params()));
        self.adopted = Some(self.params());
        true
    }

    /// Shell-level release fallback: completes a drag even when the UV tab is
    /// hidden and its handle widget cannot emit `drag_stopped`. A second release
    /// signal is inert, and `finish_drag` still rejects external document edits.
    pub fn finish_on_primary_release(
        &mut self,
        doc: &mut Document,
        primary_released: bool,
    ) -> bool {
        primary_released && self.gesture.is_some() && self.finish_drag(doc)
    }

    fn params(&self) -> VectorParams {
        VectorParams {
            shape: self.working.clone(),
        }
    }

    fn doc_matches(&self, id: u64, before: &Option<VectorParams>, doc: &Document) -> bool {
        let current = doc
            .layers()
            .iter()
            .find(|layer| layer.id == id)
            .and_then(|layer| layer.vector.as_ref());
        match (current, before.as_ref()) {
            (None, None) => true,
            (Some(current), Some(before)) => current.shape.same_cache_key(&before.shape),
            _ => false,
        }
    }

    fn commit(&mut self, doc: &mut Document, before: VectorParams) -> bool {
        let Some(id) = self.target else {
            return false;
        };
        if self.working.same_cache_key(&before.shape) || !self.doc_matches(id, &self.adopted, doc) {
            return false;
        }
        doc.run(LayerCommand::set_vector(id, self.params()));
        self.adopted = Some(self.params());
        true
    }
}

fn finite_uv(uv: [f32; 2]) -> bool {
    uv[0].is_finite() && uv[1].is_finite()
}

fn nearest_anchor(shape: &VectorShape, uv: [f32; 2], tol: f32) -> Option<usize> {
    if !finite_uv(uv) {
        return None;
    }
    let mut best: Option<(usize, f32)> = None;
    for (index, anchor) in shape.anchors.iter().enumerate() {
        let d2 = (anchor.pos[0] - uv[0]).powi(2) + (anchor.pos[1] - uv[1]).powi(2);
        if d2 <= tol * tol && best.is_none_or(|(_, old)| d2 < old) {
            best = Some((index, d2));
        }
    }
    best.map(|(index, _)| index)
}

fn nearest_segment(shape: &VectorShape, uv: [f32; 2]) -> Option<(usize, f32)> {
    let count = if shape.closed {
        shape.anchors.len()
    } else {
        shape.anchors.len().saturating_sub(1)
    };
    let mut best: Option<(usize, f32)> = None;
    for segment in 0..count {
        let a = shape.anchors[segment].pos;
        let b = shape.anchors[(segment + 1) % shape.anchors.len()].pos;
        let ab = [b[0] - a[0], b[1] - a[1]];
        let len2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if len2 > 0.0 && len2.is_finite() {
            (((uv[0] - a[0]) * ab[0] + (uv[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dx = uv[0] - (a[0] + ab[0] * t);
        let dy = uv[1] - (a[1] + ab[1] * t);
        let d2 = dx * dx + dy * dy;
        if best.is_none_or(|(_, old)| d2 < old) {
            best = Some((segment, d2));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_core::{LayerKind, LayerStack};

    fn params(closed: bool) -> VectorParams {
        VectorParams {
            shape: VectorShape {
                closed,
                fill: [255, 0, 0, 255],
                stroke: None,
                anchors: vec![
                    Anchor {
                        pos: [0.25, 0.5],
                        in_handle: [0.20, 0.40],
                        out_handle: [0.30, 0.60],
                    },
                    Anchor::corner([0.75, 0.5]),
                ],
            },
        }
    }

    fn doc_with_shape(closed: bool) -> (Document, VectorParams) {
        let mut doc = Document::default();
        doc.run(LayerCommand::add("Path", LayerKind::Vector));
        let params = params(closed);
        doc.run(LayerCommand::set_vector(0, params.clone()));
        (doc, params)
    }

    fn tool_for(doc: &Document) -> VectorEditTool {
        let mut tool = VectorEditTool::default();
        tool.adopt(Some((0, doc.layers()[0].vector.as_ref())));
        tool
    }

    #[test]
    fn discrete_edits_are_each_one_undo_step() {
        let (mut doc, _) = doc_with_shape(false);
        let mut tool = tool_for(&doc);
        let base = doc.history.len();
        assert!(tool.append(&mut doc, [0.5, 0.75]));
        assert_eq!(doc.history.len(), base + 1);
        assert!(doc.undo());
        assert_eq!(
            doc.layers()[0].vector.as_ref().unwrap().shape.anchors.len(),
            2
        );
        assert!(doc.redo());
        assert_eq!(
            doc.layers()[0].vector.as_ref().unwrap().shape.anchors.len(),
            3
        );

        assert!(tool.insert_near_segment(&mut doc, [0.5, 0.5], 0.01));
        assert_eq!(doc.history.len(), base + 2);
        assert!(doc.undo());
        assert_eq!(
            doc.layers()[0].vector.as_ref().unwrap().shape.anchors.len(),
            3
        );
        assert!(doc.redo());

        assert!(tool.set_closed(&mut doc, true));
        assert_eq!(doc.history.len(), base + 3);
        assert!(doc.undo());
        assert!(!doc.layers()[0].vector.as_ref().unwrap().shape.closed);
        assert!(doc.redo());

        assert!(tool.delete_near(&mut doc, [0.5, 0.5], 0.01));
        assert_eq!(doc.history.len(), base + 4);
        assert!(doc.undo());
        assert_eq!(
            doc.layers()[0].vector.as_ref().unwrap().shape.anchors.len(),
            4
        );
        assert!(doc.redo());
    }

    #[test]
    fn anchor_drag_preserves_handle_deltas_and_writes_once() {
        let (mut doc, original) = doc_with_shape(false);
        let mut tool = tool_for(&doc);
        let base = doc.history.len();
        assert!(tool.begin_drag(0, VectorHandle::Anchor));
        assert!(tool.drag_to([0.40, 0.70]));
        assert_eq!(doc.history.len(), base, "preview does not journal");
        assert!(tool.finish_drag(&mut doc));
        assert_eq!(doc.history.len(), base + 1);
        let edited = &doc.layers()[0].vector.as_ref().unwrap().shape.anchors[0];
        let delta = [
            0.40 - original.shape.anchors[0].pos[0],
            0.70 - original.shape.anchors[0].pos[1],
        ];
        assert_eq!(edited.pos, [0.40, 0.70]);
        assert_eq!(
            edited.in_handle,
            [
                original.shape.anchors[0].in_handle[0] + delta[0],
                original.shape.anchors[0].in_handle[1] + delta[1],
            ]
        );
        assert_eq!(
            edited.out_handle,
            [
                original.shape.anchors[0].out_handle[0] + delta[0],
                original.shape.anchors[0].out_handle[1] + delta[1],
            ]
        );
        assert!(doc.undo());
        assert_eq!(doc.layers()[0].vector.as_ref().unwrap(), &original);
        assert!(doc.redo());
        assert_eq!(
            doc.layers()[0].vector.as_ref().unwrap().shape.anchors[0].pos,
            [0.40, 0.70]
        );
    }

    #[test]
    fn in_and_out_handles_drag_independently() {
        let (mut doc, original) = doc_with_shape(false);
        let mut tool = tool_for(&doc);
        let base = doc.history.len();
        assert!(tool.begin_drag(0, VectorHandle::In));
        assert!(tool.drag_to([0.10, 0.20]));
        assert!(tool.finish_drag(&mut doc));
        let edited = &doc.layers()[0].vector.as_ref().unwrap().shape.anchors[0];
        assert_eq!(edited.in_handle, [0.10, 0.20]);
        assert_eq!(edited.out_handle, original.shape.anchors[0].out_handle);
        assert_eq!(doc.history.len(), base + 1);
        assert!(doc.undo());
        assert_eq!(doc.layers()[0].vector.as_ref().unwrap(), &original);
        assert!(doc.redo());

        assert!(tool.begin_drag(0, VectorHandle::Out));
        assert!(tool.drag_to([0.90, 0.80]));
        assert!(tool.finish_drag(&mut doc));
        assert_eq!(
            doc.layers()[0].vector.as_ref().unwrap().shape.anchors[0].out_handle,
            [0.90, 0.80]
        );
        assert_eq!(
            doc.layers()[0].vector.as_ref().unwrap().shape.anchors[0].in_handle,
            [0.10, 0.20]
        );
        assert_eq!(doc.history.len(), base + 2);
    }

    #[test]
    fn invalid_input_and_anchor_cap_refuse_without_mutation() {
        let (mut doc, _) = doc_with_shape(false);
        let mut tool = tool_for(&doc);
        let base = doc.history.len();
        assert!(!tool.append(&mut doc, [f32::NAN, 0.5]));
        assert!(!tool.insert_near_segment(&mut doc, [0.5, f32::INFINITY], 0.1));
        assert!(!tool.delete_near(&mut doc, [f32::NAN, 0.5], 0.1));
        assert!(tool.begin_drag(0, VectorHandle::Anchor));
        assert!(!tool.drag_to([f32::NAN, 0.5]));
        assert!(tool.finish_drag(&mut doc), "a no-op drag closes cleanly");
        assert_eq!(doc.history.len(), base);

        let mut full = VectorEditTool::default();
        let params = VectorParams {
            shape: VectorShape {
                anchors: vec![Anchor::corner([0.5, 0.5]); MAX_VECTOR_ANCHORS],
                ..params(false).shape
            },
        };
        full.adopt(Some((0, Some(&params))));
        assert!(!full.append(&mut doc, [0.5, 0.5]));
        assert_eq!(doc.history.len(), base);
    }

    #[test]
    fn document_undo_redo_adopts_when_not_dragging() {
        let (mut doc, original) = doc_with_shape(false);
        let mut tool = tool_for(&doc);
        let mut changed = original.clone();
        changed.shape.anchors[0].pos = [0.9, 0.1];
        doc.run(LayerCommand::set_vector(0, changed.clone()));
        tool.adopt(Some((0, doc.layers()[0].vector.as_ref())));
        assert_eq!(tool.shape(), &changed.shape);
        assert!(doc.undo());
        tool.adopt(Some((0, doc.layers()[0].vector.as_ref())));
        assert_eq!(tool.shape(), &original.shape);
    }

    #[test]
    fn external_mid_gesture_edit_is_not_clobbered() {
        let (mut doc, original) = doc_with_shape(false);
        let mut tool = tool_for(&doc);
        assert!(tool.begin_drag(0, VectorHandle::Anchor));
        assert!(tool.drag_to([0.9, 0.9]));
        let mut external = original.clone();
        external.shape.anchors[0].pos = [0.1, 0.2];
        doc.run(LayerCommand::set_vector(0, external.clone()));
        let base = doc.history.len();
        assert!(!tool.finish_drag(&mut doc));
        assert_eq!(doc.history.len(), base);
        assert_eq!(doc.layers()[0].vector.as_ref().unwrap(), &external);
        tool.adopt(Some((0, doc.layers()[0].vector.as_ref())));
        assert_eq!(tool.shape(), &external.shape);
    }

    #[test]
    fn no_net_change_writes_no_history() {
        let (mut doc, _) = doc_with_shape(false);
        let mut tool = tool_for(&doc);
        let base = doc.history.len();
        assert!(tool.begin_drag(0, VectorHandle::Anchor));
        assert!(tool.drag_to([0.25, 0.5]));
        assert!(tool.finish_drag(&mut doc));
        assert_eq!(doc.history.len(), base);
    }

    #[test]
    fn primary_release_finishes_without_uv_and_is_idempotent() {
        let (mut doc, _) = doc_with_shape(false);
        let mut tool = tool_for(&doc);
        let base = doc.history.len();
        assert!(tool.begin_drag(0, VectorHandle::Anchor));
        assert!(tool.drag_to([0.4, 0.7]));
        assert!(!tool.finish_on_primary_release(&mut doc, false));
        assert!(tool.has_gesture());
        assert!(tool.finish_on_primary_release(&mut doc, true));
        assert!(!tool.has_gesture());
        assert_eq!(doc.history.len(), base + 1);
        assert!(!tool.finish_on_primary_release(&mut doc, true));
        assert_eq!(doc.history.len(), base + 1);
    }

    #[test]
    fn exact_handle_delete_removes_its_anchor_as_one_undo_step() {
        let (mut doc, original) = doc_with_shape(false);
        let mut tool = tool_for(&doc);
        let base = doc.history.len();
        assert!(tool.delete_anchor(&mut doc, 1));
        assert_eq!(doc.history.len(), base + 1);
        assert_eq!(
            doc.layers()[0].vector.as_ref().unwrap().shape.anchors.len(),
            1
        );
        assert!(doc.undo());
        assert_eq!(doc.layers()[0].vector.as_ref().unwrap(), &original);
    }

    #[test]
    fn fresh_vector_layer_accepts_first_anchor_and_undo_restores_none() {
        let mut doc = Document::default();
        doc.run(LayerCommand::add("Path", LayerKind::Vector));
        assert!(doc.layers()[0].vector.is_none());
        let mut tool = VectorEditTool::default();
        tool.adopt(Some((0, None)));
        assert!(tool.is_editing());
        assert_eq!(tool.shape().fill, [255, 255, 255, 255]);
        let base = doc.history.len();
        assert!(tool.append(&mut doc, [0.5, 0.5]));
        assert_eq!(doc.history.len(), base + 1);
        assert_eq!(
            doc.layers()[0].vector.as_ref().unwrap().shape.anchors.len(),
            1
        );
        assert!(doc.undo());
        assert!(doc.layers()[0].vector.is_none());
        tool.adopt(Some((0, doc.layers()[0].vector.as_ref())));
        assert!(tool.is_editing());
        assert!(tool.shape().anchors.is_empty());
        assert!(doc.redo());
        tool.adopt(Some((0, doc.layers()[0].vector.as_ref())));
        assert_eq!(tool.shape().anchors.len(), 1);
    }

    #[test]
    fn empty_stack_adoption_is_safe() {
        let mut tool = VectorEditTool::default();
        let stack = LayerStack::new();
        assert!(stack.layers.is_empty());
        tool.adopt(None);
        assert!(!tool.is_editing());
    }
}
