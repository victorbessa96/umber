//! The paint-path tool: the editable polyline feeding [`PaintPath`] replays
//! (§2 line 26's P1 row).
//!
//! Session state only (like [`PolygonFillTool`](crate::fill_layer::PolygonFillTool)):
//! never serialized, never journaled. While [`Self::is_armed`], primary
//! clicks in the UV view feed this tool (append / insert on segment click)
//! instead of painting — the same capture convention as the polygon-fill
//! lasso — point handles drag, right-click deletes, and the selected
//! point's size/opacity handles live in the 2D UV view beside the panel
//! sliders here. Replay and filled-path dispatch through
//! [`PaintState::replay_path`](crate::paint_state::PaintState::replay_path)
//! / `replay_filled_path` between strokes (a live stroke is never split).
//!
//! The op mode is paint / erase / smudge ([`PathOpKind`]); each mode keeps
//! its own params while another is selected (the brush panel's per-tool
//! retention pattern), and [`Self::path_op`] builds the replay op from the
//! mode + its params. Clone is not a path mode (see
//! `umber_brush::path::PathOp`).

use umber_brush::{
    path::{PaintPath, PathOp},
    retouch::{EraseParams, SmudgeParams},
    BrushTool,
};
use umber_mesh::FillMask;

use crate::i18n::tr;
use crate::paint_state::PaintState;

/// The replay op mode (the panel picker; params live beside it so a mode
/// switch never discards the other modes' settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathOpKind {
    /// The stamp (the GPU splat pass).
    #[default]
    Paint,
    /// The CPU eraser op.
    Erase,
    /// The CPU smudge op.
    Smudge,
}

impl PathOpKind {
    /// The panel combo's i18n key for this mode.
    pub fn label_key(self) -> &'static str {
        match self {
            PathOpKind::Paint => "path.op-paint",
            PathOpKind::Erase => "path.op-erase",
            PathOpKind::Smudge => "path.op-smudge",
        }
    }
}

/// The paint-path edit session: the path, the op mode + per-mode params,
/// the capture arm, and the view selection.
#[derive(Debug, Clone, Default)]
pub struct PaintPathTool {
    path: PaintPath,
    kind: PathOpKind,
    smudge: SmudgeParams,
    eraser: EraseParams,
    armed: bool,
    selected: Option<usize>,
}

impl PaintPathTool {
    /// The path under edit.
    pub fn path(&self) -> &PaintPath {
        &self.path
    }

    /// The op mode under edit.
    pub fn kind(&self) -> PathOpKind {
        self.kind
    }

    /// Switches the op mode (params of every mode are kept).
    pub fn set_kind(&mut self, kind: PathOpKind) {
        self.kind = kind;
    }

    /// The smudge params the smudge mode replays with.
    pub fn smudge(&self) -> SmudgeParams {
        self.smudge
    }

    /// The eraser params the erase mode replays with.
    pub fn eraser(&self) -> EraseParams {
        self.eraser
    }

    /// The replay op for the current mode + its params.
    pub fn path_op(&self) -> PathOp {
        match self.kind {
            PathOpKind::Paint => PathOp::Paint,
            PathOpKind::Erase => PathOp::Erase(self.eraser),
            PathOpKind::Smudge => PathOp::Smudge(self.smudge),
        }
    }

    /// The brush tool a replay runs as (the session's existing consumer
    /// branch — paint stages splats, erase/smudge stamp the CPU mirror).
    pub fn brush_tool(&self) -> BrushTool {
        self.path_op().to_brush_tool()
    }

    /// Whether UV-view clicks feed this tool instead of painting.
    pub fn is_armed(&self) -> bool {
        self.armed
    }

    /// Arms or disarms the click capture.
    pub fn set_armed(&mut self, armed: bool) {
        self.armed = armed;
    }

    /// The captured path, in replay order.
    pub fn points(&self) -> &[umber_brush::path::PathPoint] {
        self.path.points()
    }

    /// How many points are captured.
    pub fn point_count(&self) -> usize {
        self.path.len()
    }

    /// Whether any point is captured.
    pub fn has_points(&self) -> bool {
        !self.path.is_empty()
    }

    /// The view selection (the point the handles edit).
    pub fn selected(&self) -> Option<usize> {
        self.selected.filter(|i| *i < self.path.len())
    }

    /// Selects `index` (`None` clears; out-of-bounds clears too).
    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index.filter(|i| *i < self.path.len());
    }

    /// Whether the path seals last→first on replay.
    pub fn is_closed(&self) -> bool {
        self.path.is_closed()
    }

    /// Arms or disarms the sealing segment.
    pub fn set_closed(&mut self, closed: bool) {
        self.path.set_closed(closed);
    }

    /// Appends a point at full pressure / default size / full opacity:
    /// only while armed; non-finite clicks and the point cap refuse
    /// (`false`). The new point becomes the selection.
    pub fn push_point(&mut self, uv: [f32; 2]) -> bool {
        if !self.armed {
            return false;
        }
        if !self.path.push_point(uv, 1.0, 1.0, 1.0) {
            return false;
        }
        self.selected = Some(self.path.len() - 1);
        true
    }

    /// Inserts the clicked UV into its nearest segment (the segment-click
    /// affordance): only while armed with ≥ 2 points and within `tol` (UV
    /// units) of a segment — the closing edge counts while closed. Past
    /// the cap or with no segment near, `false` (the caller appends
    /// instead). The inserted point becomes the selection.
    pub fn insert_near_segment(&mut self, uv: [f32; 2], tol: f32) -> bool {
        if !self.armed || self.path.len() < 2 {
            return false;
        }
        if !uv[0].is_finite() || !uv[1].is_finite() {
            return false;
        }
        if !(tol >= 0.0) {
            return false;
        }
        let Some((seg, _)) = nearest_segment(self.path.points(), self.path.is_closed(), uv) else {
            return false;
        };
        let (dist2, _) = dist_to_segment_t(
            uv,
            seg_a(self.path.points(), seg),
            seg_b(self.path.points(), seg),
        );
        if dist2 > tol * tol {
            return false;
        }
        // The closing edge inserts at the end (keeps click order); every
        // other segment inserts after its start index.
        let at = if self.path.is_closed() && seg == self.path.len() - 1 {
            self.path.len()
        } else {
            seg + 1
        };
        if !self.path.insert_point(at, uv, 1.0, 1.0, 1.0) {
            return false;
        }
        self.selected = Some(at);
        true
    }

    /// Deletes the nearest point within `tol` (UV units) of `uv` — the
    /// right-click affordance. The selection clears when it pointed at
    /// (or past) the removed point.
    pub fn delete_near(&mut self, uv: [f32; 2], tol: f32) -> bool {
        let Some(index) = point_near(self.path.points(), uv, tol) else {
            return false;
        };
        if !self.path.remove_point(index) {
            return false;
        }
        self.selected = match self.selected {
            Some(sel) if sel == index => None,
            Some(sel) if sel > index => Some(sel - 1),
            sel => sel.filter(|i| *i < self.path.len()),
        };
        true
    }

    /// The nearest captured point within `tol` (UV units) of `uv`, if any
    /// (the view's select hit-test).
    pub fn point_near_index(&self, uv: [f32; 2], tol: f32) -> Option<usize> {
        point_near(self.path.points(), uv, tol)
    }

    /// Moves the point at `index` (the drag affordance); `false` on
    /// out-of-bounds or non-finite coordinates.
    pub fn move_point(&mut self, index: usize, uv: [f32; 2]) -> bool {
        self.path.move_point(index, uv)
    }

    /// Restyles the point at `index`; `false` when out of bounds.
    pub fn restyle_point(&mut self, index: usize, pressure: f32, size: f32, opacity: f32) -> bool {
        self.path.restyle_point(index, pressure, size, opacity)
    }

    /// Sets the selected point's size (the view's size handle + the panel
    /// slider share this); `false` with no selection.
    pub fn set_selected_size(&mut self, size: f32) -> bool {
        match self.selected() {
            Some(i) => {
                let p = &self.path.points()[i];
                self.path.restyle_point(i, p.pressure, size, p.opacity)
            }
            None => false,
        }
    }

    /// Sets the selected point's opacity; `false` with no selection.
    pub fn set_selected_opacity(&mut self, opacity: f32) -> bool {
        match self.selected() {
            Some(i) => {
                let p = &self.path.points()[i];
                self.path.restyle_point(i, p.pressure, p.size, opacity)
            }
            None => false,
        }
    }

    /// Drops the whole capture (the closed flag is kept — it belongs to
    /// the session, like the lasso tool's mode).
    pub fn clear(&mut self) {
        self.path.clear_points();
        self.selected = None;
    }

    /// Whether the path can replay as a stroke (≥ 2 points).
    pub fn can_replay(&self) -> bool {
        self.path.can_replay()
    }

    /// Whether the path can fill (closed, ≥ 3 points).
    pub fn can_fill(&self) -> bool {
        self.path.can_fill()
    }

    /// The closed-loop mask for the even-odd rasterizer (`None` unless
    /// [`Self::can_fill`]) — the SAME `FillMask::polygon` the lasso
    /// builds, so the boundary-inclusive parity rule is shared.
    pub fn build_fill_mask(&self) -> Option<FillMask> {
        self.can_fill()
            .then(|| FillMask::polygon(self.path.loop_uv()))
    }

    /// Replays the path on `paint` as one stroke in the current op mode.
    /// `false` (nothing staged) with fewer than 2 points or while a live
    /// stroke runs — a replay never splits a stroke.
    pub fn replay_on(&self, paint: &mut PaintState) -> bool {
        if !self.can_replay() {
            return false;
        }
        paint.replay_path(&self.path, &self.path_op())
    }

    /// Stamps the closed path's interior on `paint` through the polygon
    /// rasterizer + the existing stroke pipeline. `false` unless the path
    /// can fill or while a live stroke runs.
    pub fn fill_on(&self, paint: &mut PaintState) -> bool {
        if !self.can_fill() {
            return false;
        }
        paint.replay_filled_path(&self.path, &self.path_op())
    }
}

/// The nearest captured point within `tol` (UV units) of `uv`, if any.
fn point_near(points: &[umber_brush::path::PathPoint], uv: [f32; 2], tol: f32) -> Option<usize> {
    if !uv[0].is_finite() || !uv[1].is_finite() || !(tol >= 0.0) {
        return None;
    }
    let mut best: Option<(usize, f32)> = None;
    for (i, p) in points.iter().enumerate() {
        let dx = p.uv[0] - uv[0];
        let dy = p.uv[1] - uv[1];
        let d2 = dx * dx + dy * dy;
        if d2 <= tol * tol && best.is_none_or(|(_, b)| d2 < b) {
            best = Some((i, d2));
        }
    }
    best.map(|(i, _)| i)
}

/// The nearest path segment to `uv` as `(segment index, t)`, or `None`
/// with fewer than 2 points. Segments run `i → i + 1`; while `closed`
/// (and ≥ 3 points) the sealing edge `len - 1 → 0` is segment `len - 1`.
fn nearest_segment(
    points: &[umber_brush::path::PathPoint],
    closed: bool,
    uv: [f32; 2],
) -> Option<(usize, f32)> {
    if points.len() < 2 || !uv[0].is_finite() || !uv[1].is_finite() {
        return None;
    }
    let mut count = points.len() - 1;
    if closed && points.len() >= 3 {
        count += 1;
    }
    let mut best: Option<(usize, f32, f32)> = None;
    for seg in 0..count {
        let (dist2, t) = dist_to_segment_t(uv, seg_a(points, seg), seg_b(points, seg));
        if best.is_none_or(|(_, _, b)| dist2 < b) {
            best = Some((seg, t, dist2));
        }
    }
    best.map(|(seg, t, _)| (seg, t))
}

fn seg_a(points: &[umber_brush::path::PathPoint], seg: usize) -> [f32; 2] {
    points[seg % points.len()].uv
}

fn seg_b(points: &[umber_brush::path::PathPoint], seg: usize) -> [f32; 2] {
    points[(seg + 1) % points.len()].uv
}

/// Squared distance from `p` to the segment `a–b` plus the closest-point
/// parameter `t` (clamped to the segment; a degenerate edge reads `t = 0`).
fn dist_to_segment_t(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> (f32, f32) {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    if !(len2 > 0.0) {
        let dx = p[0] - a[0];
        let dy = p[1] - a[1];
        return (dx * dx + dy * dy, 0.0);
    }
    let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0);
    let dx = p[0] - (a[0] + ab[0] * t);
    let dy = p[1] - (a[1] + ab[1] * t);
    (dx * dx + dy * dy, t)
}

/// The path-tool rows of the Properties panel: the armed toggle, the
/// three-mode picker, the mode's params, the closed flag, the capture
/// status, the selected point's size/opacity sliders, and the Replay /
/// Fill / Clear buttons.
///
/// Replay dispatches through [`PaintPathTool::replay_on`] (one stroke
/// through the session pipeline; a retouch replay lands as one undo
/// entry); every other widget edits the session tool only. All strings go
/// through [`tr`](crate::i18n::tr).
pub fn path_tool_rows(ui: &mut egui::Ui, paint: Option<&mut PaintState>, tool: &mut PaintPathTool) {
    ui.add_space(4.0);
    ui.strong(tr("path.tool"));
    ui.horizontal(|ui| {
        let mut armed = tool.is_armed();
        ui.checkbox(&mut armed, tr("path.armed"));
        tool.set_armed(armed);
    });
    ui.label(tr("path.hint"));
    // The three-mode picker (keys as literals so the i18n completeness
    // scan sees every call site).
    let mut kind = tool.kind();
    egui::ComboBox::from_id_salt("path-op-mode")
        .selected_text(tr(kind.label_key()))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut kind, PathOpKind::Paint, tr("path.op-paint"));
            ui.selectable_value(&mut kind, PathOpKind::Erase, tr("path.op-erase"));
            ui.selectable_value(&mut kind, PathOpKind::Smudge, tr("path.op-smudge"));
        });
    tool.set_kind(kind);
    // The mode's params (shared tr keys with the brush panel's tool rows).
    match tool.kind() {
        PathOpKind::Paint => {}
        PathOpKind::Smudge => {
            let mut params = tool.smudge();
            if ui
                .add(
                    egui::Slider::new(&mut params.strength, 0.0..=1.0)
                        .text(tr("brush.smudge-strength")),
                )
                .changed()
            {
                tool.smudge = params;
            }
            if ui
                .add(egui::Slider::new(&mut params.smear, 0.0..=1.0).text(tr("brush.smudge-smear")))
                .changed()
            {
                tool.smudge = params;
            }
        }
        PathOpKind::Erase => {
            let mut channels = tool.eraser().channels;
            ui.label(tr("brush.erase-channels"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut channels.r, "R");
                ui.checkbox(&mut channels.g, "G");
                ui.checkbox(&mut channels.b, "B");
                ui.checkbox(&mut channels.a, "A");
            });
            if channels != tool.eraser().channels {
                tool.eraser.channels = channels;
            }
        }
    }
    let mut closed = tool.is_closed();
    ui.checkbox(&mut closed, tr("path.closed"));
    tool.set_closed(closed);
    ui.label(format!("{}: {}", tr("path.points"), tool.point_count()));
    // The selected point's per-vertex style (the UV view's handles share
    // these through set_selected_size / set_selected_opacity).
    if let Some(selected) = tool.selected() {
        if let Some(point) = tool.points().get(selected).copied() {
            ui.label(format!("{}: {selected}", tr("path.selected")));
            let mut size = point.size.clamp(
                umber_brush::path::PATH_MIN_SIZE,
                umber_brush::path::PATH_MAX_SIZE,
            );
            if ui
                .add(
                    egui::Slider::new(
                        &mut size,
                        umber_brush::path::PATH_MIN_SIZE..=umber_brush::path::PATH_MAX_SIZE,
                    )
                    .text(tr("path.size")),
                )
                .changed()
            {
                tool.set_selected_size(size);
            }
            let mut opacity = point.opacity.clamp(0.0, 1.0);
            if ui
                .add(egui::Slider::new(&mut opacity, 0.0..=1.0).text(tr("path.opacity")))
                .changed()
            {
                tool.set_selected_opacity(opacity);
            }
        }
    }
    ui.horizontal(|ui| {
        let can_replay = paint.is_some() && tool.can_replay();
        if ui
            .add_enabled(can_replay, egui::Button::new(tr("path.replay")))
            .clicked()
        {
            if let Some(paint) = paint.as_deref_mut() {
                tool.replay_on(paint);
            }
        }
        let can_fill = paint.is_some() && tool.can_fill();
        if ui
            .add_enabled(can_fill, egui::Button::new(tr("path.fill")))
            .clicked()
        {
            if let Some(paint) = paint.as_deref_mut() {
                tool.fill_on(paint);
            }
        }
        if ui
            .add_enabled(tool.has_points(), egui::Button::new(tr("path.clear")))
            .clicked()
        {
            tool.clear();
        }
    });
    if paint.is_none() {
        ui.label(tr("path.no-paint"));
    } else if !tool.can_replay() {
        ui.label(tr("path.need-points"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_mesh::polygon_fill::{point_in_polygon, rasterize_polygon_mask};

    fn armed_tool() -> PaintPathTool {
        let mut tool = PaintPathTool::default();
        tool.set_armed(true);
        tool
    }

    fn square_tool() -> PaintPathTool {
        let mut tool = armed_tool();
        for uv in [[0.2, 0.2], [0.8, 0.2], [0.8, 0.8], [0.2, 0.8]] {
            assert!(tool.push_point(uv));
        }
        tool
    }

    #[test]
    fn capture_gates_on_armed_and_finite() {
        let mut tool = PaintPathTool::default();
        assert!(!tool.push_point([0.5, 0.5]), "disarmed captures nothing");
        tool.set_armed(true);
        assert!(!tool.push_point([f32::NAN, 0.5]));
        assert!(tool.push_point([0.5, 0.5]));
        assert_eq!(tool.selected(), Some(0));
        assert!(!tool.can_replay(), "one point never replays");
        assert!(tool.push_point([0.7, 0.5]));
        assert!(tool.can_replay());
        assert!(!tool.can_fill(), "open paths never fill");
    }

    #[test]
    fn segment_click_inserts_and_right_click_deletes() {
        let mut tool = square_tool();
        // Click near the middle of segment 0 (bottom edge): inserts at 1.
        assert!(tool.insert_near_segment([0.5, 0.21], 0.05));
        assert_eq!(tool.point_count(), 5);
        assert_eq!(tool.points()[1].uv, [0.5, 0.21]);
        assert_eq!(tool.selected(), Some(1));
        // Far from any segment: refused (the view appends instead).
        assert!(!tool.insert_near_segment([0.5, 0.5], 0.01));
        assert_eq!(tool.point_count(), 5);
        // Right-click near the inserted point deletes it; the selection
        // pointed at it, so it clears.
        assert!(tool.delete_near([0.5, 0.21], 0.05));
        assert_eq!(tool.point_count(), 4);
        assert_eq!(tool.selected(), None);
        // Deleting ahead of the selection shifts it down.
        tool.select(Some(3));
        assert!(tool.delete_near([0.2, 0.2], 0.05));
        assert_eq!(tool.selected(), Some(2));
        // Nothing near: refused.
        assert!(!tool.delete_near([0.5, 0.5], 0.01));
    }

    #[test]
    fn closing_edge_inserts_at_the_end() {
        let mut tool = square_tool();
        tool.set_closed(true);
        // Click near the middle of the sealing edge (left edge, x = 0.2):
        // inserts at the end, keeping click order.
        assert!(tool.insert_near_segment([0.21, 0.5], 0.05));
        assert_eq!(tool.point_count(), 5);
        assert_eq!(tool.points()[4].uv, [0.21, 0.5]);
        // Open: a click near the top edge reads segment 2 (not the
        // sealing edge, which only exists while closed) and inserts
        // after its start — the end point stays the end point.
        let mut open = square_tool();
        assert!(open.insert_near_segment([0.5, 0.79], 0.05));
        assert_eq!(open.point_count(), 5);
        assert_eq!(open.points()[3].uv, [0.5, 0.79]);
        assert_eq!(open.points()[4].uv, [0.2, 0.8]);
    }

    #[test]
    fn drag_moves_and_handles_restyle() {
        let mut tool = square_tool();
        assert!(tool.move_point(0, [0.3, 0.3]));
        assert_eq!(tool.points()[0].uv, [0.3, 0.3]);
        assert!(!tool.move_point(0, [f32::INFINITY, 0.0]));
        assert_eq!(tool.points()[0].uv, [0.3, 0.3]);
        assert!(!tool.move_point(99, [0.0, 0.0]));
        tool.select(Some(1));
        assert!(tool.set_selected_size(2.5));
        assert!(tool.set_selected_opacity(0.25));
        assert_eq!(tool.points()[1].size, 2.5);
        assert_eq!(tool.points()[1].opacity, 0.25);
        tool.select(None);
        assert!(!tool.set_selected_size(1.0));
        assert!(!tool.set_selected_opacity(1.0));
    }

    #[test]
    fn kind_switch_keeps_every_modes_params() {
        let mut tool = armed_tool();
        assert_eq!(tool.kind(), PathOpKind::Paint);
        assert_eq!(tool.path_op(), PathOp::Paint);
        assert_eq!(tool.brush_tool(), BrushTool::Paint);
        tool.set_kind(PathOpKind::Smudge);
        tool.smudge.strength = 0.9;
        tool.set_kind(PathOpKind::Erase);
        tool.eraser.channels.r = true;
        tool.set_kind(PathOpKind::Smudge);
        assert!((tool.smudge().strength - 0.9).abs() < 1e-6);
        tool.set_kind(PathOpKind::Erase);
        assert!(tool.eraser().channels.r);
        assert!(tool.path_op().is_retouch());
    }

    #[test]
    fn closed_path_fill_reuses_the_polygon_rasterizer() {
        // Deliverable 5 (closed-path fill): the tool's loop IS the lasso
        // loop — FillMask::polygon covers the center, not the outside,
        // and the tile rasterizer agrees with the point predicate texel
        // by texel (no duplicated parity math anywhere in this module).
        let mut tool = square_tool();
        assert!(!tool.can_fill());
        assert!(tool.build_fill_mask().is_none());
        tool.set_closed(true);
        assert!(tool.can_fill());
        let mask = tool.build_fill_mask().expect("closed square fills");
        assert!(mask.covers([0.5, 0.5], 0));
        assert!(!mask.covers([0.05, 0.5], 0));
        assert!(!mask.covers([0.5, 0.95], 7));
        let grid = rasterize_polygon_mask(&tool.path().loop_uv(), 1001, 8, 8);
        assert_eq!(grid.len(), 64);
        for y in 0..8 {
            for x in 0..8 {
                let uv = [(x as f32 + 0.5) / 8.0, 1.0 - (y as f32 + 0.5) / 8.0];
                assert_eq!(
                    grid[y * 8 + x],
                    point_in_polygon(uv, &tool.path().loop_uv()),
                    "texel ({x},{y})"
                );
            }
        }
        // The square loop [0.2,0.8]² covers exactly the middle texels
        // (centers (x+0.5)/8 in [0.2,0.8] → x in {2,3,4,5}; no center
        // sits on the boundary, so the inclusive rule is unexercised).
        for y in 0..8 {
            for x in 0..8 {
                let inside = (2..6).contains(&x) && (2..6).contains(&y);
                assert_eq!(grid[y * 8 + x], inside, "texel ({x},{y})");
            }
        }
    }
}
