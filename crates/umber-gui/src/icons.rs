//! The icon atlas: lucide glyphs, pre-rasterized, packed into one
//! texture per family (docs/specs/gui-audit-and-roadmap.md §2.3).
//!
//! v1 ships the atlas MODEL: the name→rect registry, the grid packer, the
//! texture-handle builder, and the [`icon`] / [`icon_button`] widget fns
//! that draw from the texture by rect.
//!
//! # Follow-up `icon-raster` (the build step)
//!
//! The SVG→RGBA rasterization (lucide static SVGs → `resvg`, per family at
//! 1× and 2× for DPI) is a build-step follow-up. Its output feeds
//! [`AtlasBuilder::push`]. The names below are lucide icon ids. Verify them
//! against the lucide release the build step pins.
//!
//! An unknown name, or an atlas whose texture is not built yet, draws a
//! faint placeholder box. It never panics, so a missing glyph cannot take
//! down a panel.

use std::collections::HashMap;

use egui::{
    Color32, ColorImage, Rect, Response, Sense, Stroke, StrokeKind, TextureHandle, Ui, Vec2,
};

use crate::theme;

/// The atlas family. Each family has its own atlas and logical size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconFamily {
    /// The tool row (20px).
    Tool,
    /// Panel headers / the top strip (16px).
    Panel,
}

impl IconFamily {
    /// Logical (points) edge length. Density does not change it.
    /// Icons are already compact, and DPI is handled by rasterizing at
    /// `pixels_per_point`.
    pub fn logical_px(self) -> f32 {
        match self {
            IconFamily::Tool => 20.0,
            IconFamily::Panel => 16.0,
        }
    }
}

/// Lucide ids of the tool-row family (§2.3: paint / eraser / smudge /
/// clone / fill / path / polygon / mask / symmetry).
pub const TOOL_ICONS: [&str; 10] = [
    "paintbrush",
    "eraser",
    "fingerprint", // smudge
    "stamp",       // clone
    "pipette",     // the eyedropper (Pick) — placeholder until the lucide
    // raster step lands (unknown ids draw a box, never panic)
    "paint-bucket",
    "spline", // path
    "pentagon",
    "square-dashed", // mask
    "flip-horizontal-2",
];

/// Lucide ids of the top-strip family (file / edit / undo / redo / view /
/// panel toggles).
pub const PANEL_ICONS: [&str; 8] = [
    "file",
    "folder-open",
    "save",
    "pencil",
    "undo-2",
    "redo-2",
    "eye",
    "panel-left",
];

/// One icon's pixel rect inside the atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconRect {
    /// Left, px.
    pub x: u32,
    /// Top, px.
    pub y: u32,
    /// Width, px.
    pub w: u32,
    /// Height, px.
    pub h: u32,
}

/// A packed atlas: the registry plus (once built) the GPU texture.
pub struct IconAtlas {
    size: [u32; 2],
    entries: HashMap<String, IconRect>,
    texture: Option<TextureHandle>,
}

impl IconAtlas {
    /// An atlas with no icons. Every lookup misses, so callers draw
    /// placeholders. Used until the `icon-raster` step lands.
    pub fn empty() -> Self {
        IconAtlas {
            size: [0, 0],
            entries: HashMap::new(),
            texture: None,
        }
    }

    /// The atlas pixel size.
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// The pixel rect of `name`.
    pub fn rect(&self, name: &str) -> Option<IconRect> {
        self.entries.get(name).copied()
    }

    /// Whether `name` is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// The normalized UV rect of `name` (0..1 over the atlas).
    pub fn uv(&self, name: &str) -> Option<Rect> {
        let r = self.rect(name)?;
        uv_of(r, self.size)
    }

    /// The texture, if built.
    pub fn texture(&self) -> Option<&TextureHandle> {
        self.texture.as_ref()
    }
}

/// Pixel rect → normalized UV. `None` for an empty atlas.
pub fn uv_of(r: IconRect, atlas: [u32; 2]) -> Option<Rect> {
    if atlas[0] == 0 || atlas[1] == 0 {
        return None;
    }
    let (w, h) = (atlas[0] as f32, atlas[1] as f32);
    Some(Rect::from_min_max(
        egui::pos2(r.x as f32 / w, r.y as f32 / h),
        egui::pos2((r.x + r.w) as f32 / w, (r.y + r.h) as f32 / h),
    ))
}

/// The grid layout of `count` cells of `cell` px: columns = ceil(sqrt(n)),
/// row-major. Returns (atlas size, cell origins).
pub fn pack_grid(count: usize, cell: u32) -> ([u32; 2], Vec<(u32, u32)>) {
    if count == 0 || cell == 0 {
        return ([0, 0], Vec::new());
    }
    let mut cols = 1usize;
    while cols * cols < count {
        cols += 1;
    }
    let rows = count.div_ceil(cols);
    let origins = (0..count)
        .map(|i| (((i % cols) as u32) * cell, ((i / cols) as u32) * cell))
        .collect();
    ([cols as u32 * cell, rows as u32 * cell], origins)
}

/// One rasterized icon waiting to be packed.
struct PendingIcon {
    name: String,
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

/// Packs rasterized icons (the `icon-raster` step's output) into a grid
/// and uploads the texture.
#[derive(Default)]
pub struct AtlasBuilder {
    icons: Vec<PendingIcon>,
}

impl AtlasBuilder {
    /// An empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one icon: `rgba` is `w*h*4` unmultiplied bytes. Panics on a
    /// size mismatch, which is a build-step bug.
    pub fn push(&mut self, name: impl Into<String>, w: u32, h: u32, rgba: Vec<u8>) -> &mut Self {
        let name = name.into();
        assert_eq!(
            rgba.len(),
            (w * h * 4) as usize,
            "icon `{name}`: rgba is {} bytes, expected {w}x{h}x4",
            rgba.len()
        );
        self.icons.push(PendingIcon { name, w, h, rgba });
        self
    }

    /// Pack into a CPU image + the registry, with no GPU work.
    /// The cell size is the largest icon edge.
    pub fn pack(&self) -> (ColorImage, HashMap<String, IconRect>) {
        let cell = self.icons.iter().map(|i| i.w.max(i.h)).max().unwrap_or(0);
        let (size, origins) = pack_grid(self.icons.len(), cell);
        let (aw, ah) = (size[0] as usize, size[1] as usize);
        let mut pixels = vec![0u8; aw * ah * 4];
        let mut entries = HashMap::with_capacity(self.icons.len());
        for (icon, &(ox, oy)) in self.icons.iter().zip(&origins) {
            for row in 0..icon.h as usize {
                let src = row * icon.w as usize * 4;
                let dst = ((oy as usize + row) * aw + ox as usize) * 4;
                let len = icon.w as usize * 4;
                pixels[dst..dst + len].copy_from_slice(&icon.rgba[src..src + len]);
            }
            entries.insert(
                icon.name.clone(),
                IconRect {
                    x: ox,
                    y: oy,
                    w: icon.w,
                    h: icon.h,
                },
            );
        }
        (
            ColorImage::from_rgba_unmultiplied([aw, ah], &pixels),
            entries,
        )
    }

    /// Pack and upload the texture.
    pub fn build(&self, ctx: &egui::Context, texture_name: &str) -> IconAtlas {
        let (image, entries) = self.pack();
        let size = [image.size[0] as u32, image.size[1] as u32];
        let texture = (size[0] > 0)
            .then(|| ctx.load_texture(texture_name, image, egui::TextureOptions::LINEAR));
        IconAtlas {
            size,
            entries,
            texture,
        }
    }
}

/// Draw `name` from `atlas` at `size` points, tinted (lucide glyphs are
/// white-on-transparent; the tint colors them).
fn paint_icon(ui: &Ui, atlas: &IconAtlas, name: &str, rect: Rect, tint: Color32) {
    match (atlas.texture(), atlas.uv(name)) {
        (Some(tex), Some(uv)) => {
            ui.painter().image(tex.id(), rect, uv, tint);
        }
        _ => {
            ui.painter().rect_stroke(
                rect.shrink(2.0),
                2.0,
                Stroke::new(1.0, theme::faint()),
                StrokeKind::Inside,
            );
        }
    }
}

/// A static icon, `size` points square.
pub fn icon(ui: &mut Ui, atlas: &IconAtlas, name: &str, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_icon(ui, atlas, name, rect, theme::text());
    }
    response
}

/// The inner padding of an icon button around its glyph.
pub const ICON_BUTTON_PAD: f32 = 4.0;

/// A clickable icon. Hover raises the background. `active` tints the
/// glyph with the accent (deepened while hovered).
pub fn icon_button(
    ui: &mut Ui,
    atlas: &IconAtlas,
    name: &str,
    size: f32,
    active: bool,
) -> Response {
    let edge = size + 2.0 * ICON_BUTTON_PAD;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(edge), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered || response.is_pointer_button_down_on() {
            ui.painter().rect_filled(rect, 3.0, theme::hover());
        }
        let tint = match (active, hovered) {
            (true, false) => theme::accent(),
            (true, true) => theme::accent_hover(),
            (false, true) => theme::text(),
            (false, false) => theme::muted(),
        };
        paint_icon(ui, atlas, name, rect.shrink(ICON_BUTTON_PAD), tint);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_square_ish_row_major() {
        let (size, origins) = pack_grid(5, 20);
        assert_eq!(size, [60, 40]); // 3 cols x 2 rows
        assert_eq!(origins, vec![(0, 0), (20, 0), (40, 0), (0, 20), (20, 20)]);
        assert_eq!(pack_grid(0, 20).0, [0, 0]);
        assert_eq!(pack_grid(9, 16).0, [48, 48]);
    }

    #[test]
    fn pack_registers_rects_and_copies_pixels() {
        let mut b = AtlasBuilder::new();
        b.push("a", 2, 2, vec![255; 16])
            .push("b", 2, 2, vec![0; 16]);
        let (image, entries) = b.pack();
        assert_eq!(image.size, [4, 2]);
        assert_eq!(
            entries["b"],
            IconRect {
                x: 2,
                y: 0,
                w: 2,
                h: 2,
            }
        );
        let uv = uv_of(entries["b"], [4, 2]).unwrap();
        assert_eq!(uv.min, egui::pos2(0.5, 0.0));
        assert_eq!(uv.max, egui::pos2(1.0, 1.0));
    }

    #[test]
    fn empty_atlas_misses_cleanly() {
        let atlas = IconAtlas::empty();
        assert!(atlas.uv("paintbrush").is_none());
        assert!(atlas.texture().is_none());
    }
}
