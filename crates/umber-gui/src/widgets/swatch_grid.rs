//! `swatch_grid` — recent colors + the SV/hue picker placeholder
//! (docs/specs/gui-audit-and-roadmap.md §3 Phase 0a; Phase 2's color
//! picker seed).
//!
//! v1: [`RecentColors`] (most-recent-first, deduped, capped), the
//! clickable [`SwatchGrid`], and [`sv_hue_placeholder`]. The placeholder
//! is a NON-interactive preview of the SV square and hue strip for one
//! hue. The interactive picker (drag handles, hex/rgb/hsv rows,
//! eyedropper) is Phase 2's `widgets/color_picker.rs`.

use egui::epaint::Mesh;
use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};

use crate::theme;

/// Default number of remembered colors.
pub const RECENT_CAPACITY: usize = 16;

/// The recent-colors list: most recent first, no duplicates.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentColors {
    colors: Vec<Color32>,
    capacity: usize,
}

impl Default for RecentColors {
    fn default() -> Self {
        Self::with_capacity(RECENT_CAPACITY)
    }
}

impl RecentColors {
    /// An empty list holding at most `capacity` colors (at least 1).
    pub fn with_capacity(capacity: usize) -> Self {
        RecentColors {
            colors: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    /// Record a use: moves an existing color to the front, else inserts
    /// it there and drops the oldest past capacity.
    pub fn push(&mut self, color: Color32) {
        self.colors.retain(|&c| c != color);
        self.colors.insert(0, color);
        self.colors.truncate(self.capacity);
    }

    /// The colors, most recent first.
    pub fn colors(&self) -> &[Color32] {
        &self.colors
    }
}

/// A grid of clickable swatches.
pub struct SwatchGrid<'a> {
    colors: &'a [Color32],
    columns: usize,
    swatch: f32,
    selected: Option<Color32>,
}

impl<'a> SwatchGrid<'a> {
    /// A grid over `colors` (8 columns, 16pt swatches).
    pub fn new(colors: &'a [Color32]) -> Self {
        SwatchGrid {
            colors,
            columns: 8,
            swatch: 16.0,
            selected: None,
        }
    }

    /// Columns per row (at least 1).
    pub fn columns(mut self, columns: usize) -> Self {
        self.columns = columns.max(1);
        self
    }

    /// Swatch edge, points.
    pub fn swatch_size(mut self, size: f32) -> Self {
        self.swatch = size;
        self
    }

    /// Ring this color with the accent (the current brush color).
    pub fn selected(mut self, color: Option<Color32>) -> Self {
        self.selected = color;
        self
    }

    /// Draw the grid. Returns the clicked color, if any.
    pub fn show(self, ui: &mut Ui) -> Option<Color32> {
        let gap = 2.0;
        let rows = self.colors.len().div_ceil(self.columns);
        let size = Vec2::new(
            self.columns as f32 * (self.swatch + gap) - gap,
            rows as f32 * (self.swatch + gap) - gap,
        )
        .max(Vec2::ZERO);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let mut clicked = None;
        let hover_pos = response.hover_pos();
        for (i, &color) in self.colors.iter().enumerate() {
            let min = rect.min
                + Vec2::new(
                    (i % self.columns) as f32 * (self.swatch + gap),
                    (i / self.columns) as f32 * (self.swatch + gap),
                );
            let cell = Rect::from_min_size(min, Vec2::splat(self.swatch));
            let hovered = hover_pos.is_some_and(|p| cell.contains(p));
            let painter = ui.painter();
            painter.rect_filled(cell, 2.0, color);
            let ring = if self.selected == Some(color) {
                Some(Stroke::new(1.5, theme::accent()))
            } else if hovered {
                Some(Stroke::new(1.0, theme::text()))
            } else {
                None
            };
            if let Some(stroke) = ring {
                painter.rect_stroke(cell, 2.0, stroke, StrokeKind::Outside);
            }
            if hovered && response.clicked() {
                clicked = Some(color);
            }
        }
        clicked
    }
}

/// Hue strip stops: red, yellow, green, cyan, blue, magenta, red.
pub const HUE_STOPS: [Color32; 7] = [
    Color32::from_rgb(255, 0, 0),
    Color32::from_rgb(255, 255, 0),
    Color32::from_rgb(0, 255, 0),
    Color32::from_rgb(0, 255, 255),
    Color32::from_rgb(0, 0, 255),
    Color32::from_rgb(255, 0, 255),
    Color32::from_rgb(255, 0, 0),
];

/// Push a quad with per-corner colors (top-left, top-right,
/// bottom-right, bottom-left) onto `mesh`.
fn quad(mesh: &mut Mesh, r: Rect, c: [Color32; 4]) {
    let base = mesh.vertices.len() as u32;
    mesh.colored_vertex(r.left_top(), c[0]);
    mesh.colored_vertex(r.right_top(), c[1]);
    mesh.colored_vertex(r.right_bottom(), c[2]);
    mesh.colored_vertex(r.left_bottom(), c[3]);
    mesh.add_triangle(base, base + 1, base + 2);
    mesh.add_triangle(base, base + 2, base + 3);
}

/// The SV/hue placeholder: an SV square for `hue` (white→hue across,
/// then transparent→black down) over a hue strip. The vertex colors
/// interpolate in gamma space, which is good enough for a preview. Phase
/// 2 replaces it with a shader-exact picker.
pub fn sv_hue_placeholder(ui: &mut Ui, hue: Color32, size: f32) -> Response {
    let strip = 10.0;
    let gap = 4.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(size, size + gap + strip), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let sv = Rect::from_min_size(rect.min, Vec2::splat(size));
    let mut mesh = Mesh::default();
    quad(&mut mesh, sv, [Color32::WHITE, hue, hue, Color32::WHITE]);
    quad(
        &mut mesh,
        sv,
        [
            Color32::TRANSPARENT,
            Color32::TRANSPARENT,
            Color32::BLACK,
            Color32::BLACK,
        ],
    );
    let top = sv.bottom() + gap;
    let seg = size / (HUE_STOPS.len() - 1) as f32;
    for (i, pair) in HUE_STOPS.windows(2).enumerate() {
        let r = Rect::from_min_size(
            Pos2::new(rect.left() + i as f32 * seg, top),
            Vec2::new(seg, strip),
        );
        quad(&mut mesh, r, [pair[0], pair[1], pair[1], pair[0]]);
    }
    let painter = ui.painter();
    painter.add(Shape::mesh(mesh));
    painter.rect_stroke(
        sv,
        0.0,
        Stroke::new(1.0, theme::raised()),
        StrokeKind::Outside,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Color32 = Color32::from_rgb(1, 2, 3);
    const B: Color32 = Color32::from_rgb(4, 5, 6);
    const C: Color32 = Color32::from_rgb(7, 8, 9);

    #[test]
    fn recent_is_mru_deduped_capped() {
        let mut r = RecentColors::with_capacity(2);
        r.push(A);
        r.push(B);
        assert_eq!(r.colors(), &[B, A]);
        r.push(A);
        assert_eq!(r.colors(), &[A, B]);
        r.push(C);
        assert_eq!(r.colors(), &[C, A]);
        assert_eq!(RecentColors::with_capacity(0).capacity, 1);
    }

    #[test]
    fn mesh_quads_are_two_triangles() {
        let mut m = Mesh::default();
        quad(
            &mut m,
            Rect::from_min_size(Pos2::ZERO, Vec2::splat(1.0)),
            [A; 4],
        );
        assert_eq!(m.vertices.len(), 4);
        assert_eq!(m.indices.len(), 6);
    }
}
