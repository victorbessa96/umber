//! The icon atlas: lucide glyphs, pre-rasterized, packed into one
//! texture per family (docs/specs/gui-audit-and-roadmap.md §2.3).
//!
//! v1 ships the atlas MODEL: the name→rect registry, the grid packer, the
//! texture-handle builder, and the [`icon`] / [`icon_button`] widget fns
//! that draw from the texture by rect.
//!
//! # The `icon-raster` build step (tool LANDED, artifacts generating)
//!
//! `tools/icon-raster` rasterizes the pinned lucide SVGs
//! (`assets/icons/lucide/`, release [`LUCIDE_VERSION`]) at 1x and 2x of each
//! family's logical size and writes the packed PNGs + `manifest.json` into
//! `assets/icons/atlas/`. [`load_baked_atlas`] reads that output back through
//! the existing [`AtlasBuilder`] path, so real glyphs replace the placeholder
//! boxes. Until the PNGs are generated (the tool runs at gate time) the only
//! atlas is [`IconAtlas::empty()`]; the app wiring that swaps the empty
//! atlas for the baked one is the named follow-up (see `toolbar.rs`' "The
//! tool strip shows placeholder glyphs" judgment call).
//!
//! # Why umber-gui decodes PNGs itself (the cycle decision)
//!
//! Reusing `umber-export::png::read_png_rgba8` was considered and rejected
//! on two independent grounds: (a) layering — this crate depends on egui
//! alone (see `lib.rs`), and `umber-export` would drag `umber-graph` plus
//! the image codec stack with it; (b) row-order semantics — that reader
//! flips to GPU row order for equirect sampling, while the atlas needs
//! top-first rows for `ColorImage`. There is no dependency cycle either way
//! (nothing depends on `umber-gui` except `umber-app`), but the reuse would
//! still flip every glyph vertically, so the loader carries its own
//! 8-bit-RGBA-only decoder ([`decode_atlas_png`]) on the `png` micro-crate.
//!
//! # Deferred (v1 boundary)
//!
//! Runtime re-raster on DPI change is NOT done: the 2x bake covers HiDPI,
//! and callers pick the scale once via `hidpi`. Regenerating the atlas
//! mid-session on `pixels_per_point` movement is a follow-up.
//!
//! An unknown name, or an atlas whose texture is not built yet, draws a
//! faint placeholder box. It never panics, so a missing glyph cannot take
//! down a panel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
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
/// `Debug` by hand: the texture handle has no derived shape and the
/// entries carry no secrets — the error paths that print this type
/// only need the size + entry count.
pub struct IconAtlas {
    size: [u32; 2],
    entries: HashMap<String, IconRect>,
    texture: Option<TextureHandle>,
}

impl std::fmt::Debug for IconAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IconAtlas")
            .field("size", &self.size)
            .field("entries", &self.entries.len())
            .field("has_texture", &self.texture.is_some())
            .finish()
    }
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

/// Pinned lucide release the baked atlas was rasterized from (the tool's
/// single source of truth re-exported for the loader's version gate).
pub const LUCIDE_VERSION: &str = "1.55.0";

/// Manifest schema version the loader accepts.
pub const ATLAS_VERSION: u32 = 1;

/// Repo-relative dir holding the baked atlas PNGs + `manifest.json`.
pub const ATLAS_DIR_PATH: &str = "assets/icons/atlas";

/// Env var overriding the runtime atlas dir.
pub const ATLAS_DIR_ENV: &str = "UMBER_ICON_ATLAS_DIR";

/// Manifest file name inside the atlas dir.
pub const MANIFEST_FILE: &str = "manifest.json";

/// One baked family at one scale: the manifest section the loader reads.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BakedFamily {
    /// Cell edge, px (logical size x scale — every rect equals this).
    pub cell: u32,
    /// Atlas image size, px.
    pub atlas: [u32; 2],
    /// Push order: declaration order minus missing. Re-pushing in this
    /// order through [`AtlasBuilder`] reproduces the tool's grid.
    pub order: Vec<String>,
    /// name -> rect.
    pub icons: HashMap<String, IconRect>,
}

/// The sidecar manifest (`assets/icons/atlas/manifest.json`).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BakedManifest {
    /// Schema version (must equal [`ATLAS_VERSION`]).
    pub atlas_version: u32,
    /// Lucide release tag (must equal [`LUCIDE_VERSION`]: a re-pin without
    /// a re-raster refuses to load rather than mixing glyph sets).
    pub lucide: String,
    /// Release URL the SVGs were fetched from (provenance).
    pub source: String,
    /// Declared ids absent from the release (placeholder boxes by design).
    #[serde(default)]
    pub missing: Vec<String>,
    /// Tool family, 1x.
    pub tool: BakedFamily,
    /// Tool family, 2x.
    pub tool2x: BakedFamily,
    /// Panel family, 1x.
    pub panel: BakedFamily,
    /// Panel family, 2x.
    pub panel2x: BakedFamily,
}

impl BakedManifest {
    /// The section for `family` at 2x (`hidpi`) or 1x.
    pub fn section(&self, family: IconFamily, hidpi: bool) -> &BakedFamily {
        match (family, hidpi) {
            (IconFamily::Tool, false) => &self.tool,
            (IconFamily::Tool, true) => &self.tool2x,
            (IconFamily::Panel, false) => &self.panel,
            (IconFamily::Panel, true) => &self.panel2x,
        }
    }
}

/// What went wrong loading a baked atlas. Missing files are errors (the
/// caller falls back to [`IconAtlas::empty`]), never panics — the
/// placeholder-box contract holds end to end.
#[derive(Debug)]
pub enum AtlasLoadError {
    /// The manifest or a PNG could not be read.
    Io {
        /// The path being touched.
        path: PathBuf,
        /// The OS error.
        message: String,
    },
    /// The manifest is not valid JSON / has the wrong shape.
    Json(String),
    /// A PNG is unreadable or not 8-bit RGBA.
    Png(String),
    /// The manifest is internally inconsistent (unknown icon in the push
    /// order, zero-sized or out-of-bounds rect).
    Structure(String),
    /// The manifest's schema version is not [`ATLAS_VERSION`].
    SchemaVersion {
        /// The loader's version.
        expected: u32,
        /// The file's version.
        found: u32,
    },
    /// The manifest was rasterized from a different lucide release.
    LucideMismatch {
        /// The loader's pin.
        expected: String,
        /// The file's pin.
        found: String,
    },
    /// The PNG's dimensions disagree with the manifest's.
    DimensionMismatch {
        /// The PNG file name (not the full path — the dir is in [`Io`]).
        file: String,
        /// The manifest's atlas size.
        manifest: [u32; 2],
        /// The decoded PNG's size.
        decoded: [u32; 2],
    },
    /// The push order names an icon with no rect entry.
    UnknownIcon {
        /// The missing name.
        name: String,
    },
}

impl std::fmt::Display for AtlasLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AtlasLoadError::Io { path, message } => {
                write!(f, "atlas io {}: {message}", path.display())
            }
            AtlasLoadError::Json(message) => write!(f, "atlas manifest json: {message}"),
            AtlasLoadError::Png(message) => write!(f, "atlas png: {message}"),
            AtlasLoadError::Structure(message) => write!(f, "atlas manifest: {message}"),
            AtlasLoadError::SchemaVersion { expected, found } => write!(
                f,
                "atlas schema v{found} != loader v{expected} (re-run icon-raster)"
            ),
            AtlasLoadError::LucideMismatch { expected, found } => write!(
                f,
                "atlas lucide {found} != loader pin {expected} (re-run icon-raster)"
            ),
            AtlasLoadError::DimensionMismatch {
                file,
                manifest,
                decoded,
            } => write!(
                f,
                "atlas {file}: manifest {manifest:?} != png {decoded:?} (re-run icon-raster)"
            ),
            AtlasLoadError::UnknownIcon { name } => {
                write!(f, "atlas manifest: `{name}` in push order but has no rect")
            }
        }
    }
}

impl std::error::Error for AtlasLoadError {}

/// The runtime atlas dir: `$UMBER_ICON_ATLAS_DIR`, else the in-tree
/// `assets/icons/atlas/` (resolved from this crate's manifest dir, the
/// `fonts_dir` pattern).
pub fn atlas_dir() -> PathBuf {
    match std::env::var_os(ATLAS_DIR_ENV) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(ATLAS_DIR_PATH),
    }
}

/// The manifest key for `family` at 2x (`hidpi`) or 1x.
pub fn manifest_key(family: IconFamily, hidpi: bool) -> &'static str {
    match (family, hidpi) {
        (IconFamily::Tool, false) => "tool",
        (IconFamily::Tool, true) => "tool2x",
        (IconFamily::Panel, false) => "panel",
        (IconFamily::Panel, true) => "panel2x",
    }
}

/// The PNG file name for `family` at 2x (`hidpi`) or 1x.
pub fn atlas_file_name(family: IconFamily, hidpi: bool) -> &'static str {
    match (family, hidpi) {
        (IconFamily::Tool, false) => "tool.png",
        (IconFamily::Tool, true) => "tool@2x.png",
        (IconFamily::Panel, false) => "panel.png",
        (IconFamily::Panel, true) => "panel@2x.png",
    }
}

/// The egui texture name for `family` at 2x (`hidpi`) or 1x.
pub fn texture_name(family: IconFamily, hidpi: bool) -> String {
    format!("icon-atlas-{}", manifest_key(family, hidpi))
}

/// Read + gate a manifest: schema version and lucide pin must match, or
/// loading refuses rather than mixing glyph sets.
pub fn load_baked_manifest_from(dir: &Path) -> Result<BakedManifest, AtlasLoadError> {
    let path = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| AtlasLoadError::Io {
        path: path.clone(),
        message: e.to_string(),
    })?;
    let manifest: BakedManifest =
        serde_json::from_str(&text).map_err(|e| AtlasLoadError::Json(e.to_string()))?;
    if manifest.atlas_version != ATLAS_VERSION {
        return Err(AtlasLoadError::SchemaVersion {
            expected: ATLAS_VERSION,
            found: manifest.atlas_version,
        });
    }
    if manifest.lucide != LUCIDE_VERSION {
        return Err(AtlasLoadError::LucideMismatch {
            expected: LUCIDE_VERSION.to_owned(),
            found: manifest.lucide.clone(),
        });
    }
    Ok(manifest)
}

/// Decode an atlas PNG into top-first RGBA8. Accepts 8-bit RGBA only (the
/// tool's exact output); anything else is an error, never a guess.
///
/// No vertical flip happens here — deliberate contrast with
/// `umber-export::png::read_png_rgba8`, which flips to GPU row order for
/// equirect sampling. egui `ColorImage` rows are top-first, so the atlas
/// must be too (see the module doc's cycle decision).
fn decode_atlas_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), AtlasLoadError> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder
        .read_info()
        .map_err(|e| AtlasLoadError::Png(e.to_string()))?;
    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    let (color, depth) = {
        let info = reader.info();
        (info.color_type, info.bit_depth)
    };
    if color != png::ColorType::Rgba || depth != png::BitDepth::Eight {
        return Err(AtlasLoadError::Png(format!(
            "want 8-bit RGBA, got {color:?} at {depth:?}"
        )));
    }
    let mut raw = vec![0u8; reader.output_buffer_size()];
    reader
        .next_frame(&mut raw)
        .map_err(|e| AtlasLoadError::Png(e.to_string()))?;
    Ok((width, height, raw))
}

/// Copy one cell out of a top-first atlas image.
fn crop_cell(atlas: &[u8], atlas_w: u32, rect: IconRect) -> Vec<u8> {
    let mut out = vec![0u8; rect.w as usize * rect.h as usize * 4];
    for row in 0..rect.h as usize {
        let src = ((rect.y as usize + row) * atlas_w as usize + rect.x as usize) * 4;
        let dst = row * rect.w as usize * 4;
        let len = rect.w as usize * 4;
        out[dst..dst + len].copy_from_slice(&atlas[src..src + len]);
    }
    out
}

/// Push one baked family through [`AtlasBuilder`] and upload the texture.
/// Cells go in manifest order, so the grid the builder packs is the grid
/// the tool wrote (uniform cells make the repack an identity).
pub fn assemble_baked_atlas(
    ctx: &egui::Context,
    name: &str,
    family: &BakedFamily,
    png_bytes: &[u8],
    file: &str,
) -> Result<IconAtlas, AtlasLoadError> {
    let (w, h, rgba) = decode_atlas_png(png_bytes)?;
    if [w, h] != family.atlas {
        return Err(AtlasLoadError::DimensionMismatch {
            file: file.to_owned(),
            manifest: family.atlas,
            decoded: [w, h],
        });
    }
    let mut builder = AtlasBuilder::new();
    for icon_name in &family.order {
        let rect = family
            .icons
            .get(icon_name)
            .ok_or_else(|| AtlasLoadError::UnknownIcon {
                name: icon_name.clone(),
            })?;
        if rect.w == 0 || rect.h == 0 {
            return Err(AtlasLoadError::Structure(format!(
                "`{icon_name}` has a zero-sized rect"
            )));
        }
        if rect.x + rect.w > w || rect.y + rect.h > h {
            return Err(AtlasLoadError::Structure(format!(
                "`{icon_name}` rect {rect:?} falls outside {w}x{h}"
            )));
        }
        builder.push(
            icon_name.clone(),
            rect.w,
            rect.h,
            crop_cell(&rgba, w, *rect),
        );
    }
    Ok(builder.build(ctx, name))
}

/// Load one baked family from `dir` (manifest + PNG) and upload it.
pub fn load_baked_atlas_from(
    ctx: &egui::Context,
    family: IconFamily,
    hidpi: bool,
    dir: &Path,
) -> Result<IconAtlas, AtlasLoadError> {
    let manifest = load_baked_manifest_from(dir)?;
    let file = atlas_file_name(family, hidpi);
    let path = dir.join(file);
    let bytes = std::fs::read(&path).map_err(|e| AtlasLoadError::Io {
        path: path.clone(),
        message: e.to_string(),
    })?;
    assemble_baked_atlas(
        ctx,
        &texture_name(family, hidpi),
        manifest.section(family, hidpi),
        &bytes,
        file,
    )
}

/// Load one baked family from [`atlas_dir`] and upload it.
pub fn load_baked_atlas(
    ctx: &egui::Context,
    family: IconFamily,
    hidpi: bool,
) -> Result<IconAtlas, AtlasLoadError> {
    load_baked_atlas_from(ctx, family, hidpi, &atlas_dir())
}

/// Load one baked family from the binary-embedded atlas (the
/// `embedded-icons` feature) and upload it.
///
/// The embedded bytes are the same files the tool writes, baked in with
/// `include_bytes!` (the `fonts.rs` `embedded-fonts` pattern). The feature
/// is off until the PNGs exist — `include_bytes!` of a missing file is a
/// compile error — so this whole item only compiles with the feature on.
#[cfg(feature = "embedded-icons")]
pub fn load_embedded_atlas(
    ctx: &egui::Context,
    family: IconFamily,
    hidpi: bool,
) -> Result<IconAtlas, AtlasLoadError> {
    let manifest_str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/icons/atlas/manifest.json"
    ));
    let manifest: BakedManifest =
        serde_json::from_str(manifest_str).map_err(|e| AtlasLoadError::Json(e.to_string()))?;
    if manifest.atlas_version != ATLAS_VERSION {
        return Err(AtlasLoadError::SchemaVersion {
            expected: ATLAS_VERSION,
            found: manifest.atlas_version,
        });
    }
    if manifest.lucide != LUCIDE_VERSION {
        return Err(AtlasLoadError::LucideMismatch {
            expected: LUCIDE_VERSION.to_owned(),
            found: manifest.lucide.clone(),
        });
    }
    let png_bytes: &[u8] = match manifest_key(family, hidpi) {
        "tool" => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/icons/atlas/tool.png"
        )) as &[u8],
        "tool2x" => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/icons/atlas/tool@2x.png"
        )) as &[u8],
        "panel" => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/icons/atlas/panel.png"
        )) as &[u8],
        _ => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/icons/atlas/panel@2x.png"
        )) as &[u8],
    };
    assemble_baked_atlas(
        ctx,
        &texture_name(family, hidpi),
        manifest.section(family, hidpi),
        png_bytes,
        atlas_file_name(family, hidpi),
    )
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

    /// Encode `rgba` as 8-bit RGBA PNG (the test-side twin of the tool's
    /// writer — same `png` crate, same settings).
    fn encode_test_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header encodes");
            writer.write_image_data(rgba).expect("pixels encode");
        }
        out
    }

    #[test]
    fn crop_cell_copies_exact_texels() {
        // 4x2 top-first atlas with distinct bytes per texel.
        let mut atlas = Vec::new();
        for texel in 0u8..8 {
            let base = texel * 4;
            atlas.extend_from_slice(&[base, base + 1, base + 2, 255]);
        }
        let cell = crop_cell(
            &atlas,
            4,
            IconRect {
                x: 2,
                y: 0,
                w: 2,
                h: 2,
            },
        );
        // Texels 2,3 of row 0 and texels 6,7 of row 1.
        assert_eq!(
            cell,
            vec![8, 9, 10, 255, 12, 13, 14, 255, 24, 25, 26, 255, 28, 29, 30, 255]
        );
    }

    #[test]
    fn decode_keeps_top_first_row_order() {
        // 1x2, stored row 0 red / row 1 green. umber-export's reader flips
        // to GPU row order; the atlas must NOT (egui rows are top-first).
        let stored = vec![255, 0, 0, 255, 0, 255, 0, 255];
        let png = encode_test_png(1, 2, &stored);
        let (w, h, rgba) = decode_atlas_png(&png).expect("decodes");
        assert_eq!((w, h), (1, 2));
        assert_eq!(&rgba[0..4], &[255, 0, 0, 255], "row 0 stays red");
        assert_eq!(&rgba[4..8], &[0, 255, 0, 255], "row 1 stays green");
    }

    #[test]
    fn decode_rejects_non_rgba() {
        // A valid PNG with the wrong color type is an error, never a guess.
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header encodes");
            writer.write_image_data(&[1, 2, 3]).expect("pixels encode");
        }
        let err = decode_atlas_png(&out).unwrap_err();
        assert!(matches!(err, AtlasLoadError::Png(_)));
    }

    /// One synthetic baked family: 4x2 atlas, red cell "a" + green "b".
    fn synthetic_family() -> (BakedFamily, Vec<u8>) {
        let rgba: Vec<u8> = vec![
            255, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 0,
            0, 255, 0, 255, 0, 255, 0, 255, 0, 255,
        ];
        let png = encode_test_png(4, 2, &rgba);
        let mut icons = HashMap::new();
        icons.insert(
            "a".to_owned(),
            IconRect {
                x: 0,
                y: 0,
                w: 2,
                h: 2,
            },
        );
        icons.insert(
            "b".to_owned(),
            IconRect {
                x: 2,
                y: 0,
                w: 2,
                h: 2,
            },
        );
        (
            BakedFamily {
                cell: 2,
                atlas: [4, 2],
                order: vec!["a".to_owned(), "b".to_owned()],
                icons,
            },
            png,
        )
    }

    #[test]
    fn assemble_builds_texture_through_the_builder_path() {
        let (family, png) = synthetic_family();
        let ctx = egui::Context::default();
        let atlas =
            assemble_baked_atlas(&ctx, "test-atlas", &family, &png, "test.png").expect("assembles");
        assert_eq!(atlas.size(), [4, 2]);
        assert_eq!(
            atlas.rect("b"),
            Some(IconRect {
                x: 2,
                y: 0,
                w: 2,
                h: 2
            })
        );
        // The texture exists (headless Context stores it CPU-side — no GPU
        // needed for the handle), and the UVs are the builder's grid math.
        assert!(atlas.texture().is_some());
        let uv = atlas.uv("a").expect("uv of a");
        assert_eq!(uv.min, egui::pos2(0.0, 0.0));
        assert_eq!(uv.max, egui::pos2(0.5, 1.0));
    }

    #[test]
    fn assemble_rejects_dimension_drift() {
        let (mut family, png) = synthetic_family();
        family.atlas = [8, 8];
        let ctx = egui::Context::default();
        let err = assemble_baked_atlas(&ctx, "test-atlas", &family, &png, "test.png").unwrap_err();
        assert!(matches!(err, AtlasLoadError::DimensionMismatch { .. }));
    }

    #[test]
    fn assemble_rejects_unknown_push_order_name() {
        let (mut family, png) = synthetic_family();
        family.order.push("ghost".to_owned());
        let ctx = egui::Context::default();
        let err = assemble_baked_atlas(&ctx, "test-atlas", &family, &png, "test.png").unwrap_err();
        assert!(matches!(err, AtlasLoadError::UnknownIcon { .. }));
    }

    #[test]
    fn manifest_gates_reject_wrong_pin_and_schema() {
        let dir = std::env::temp_dir().join("umber-gui-icon-test-gates");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        let write = |lucide: &str, version: u32| {
            std::fs::write(
                dir.join(MANIFEST_FILE),
                format!(
                    "{{\"atlas_version\": {version}, \"lucide\": \"{lucide}\", \
                     \"source\": \"x\", \"tool\": {{\"cell\": 2, \"atlas\": [0, 0], \
                     \"order\": [], \"icons\": {{}}}}, \
                     \"tool2x\": {{\"cell\": 4, \"atlas\": [0, 0], \"order\": [], \
                     \"icons\": {{}}}}, \"panel\": {{\"cell\": 2, \"atlas\": [0, 0], \
                     \"order\": [], \"icons\": {{}}}}, \
                     \"panel2x\": {{\"cell\": 4, \"atlas\": [0, 0], \"order\": [], \
                     \"icons\": {{}}}}}}"
                ),
            )
            .expect("write manifest");
        };
        write("0.0.0", ATLAS_VERSION);
        let err = load_baked_manifest_from(&dir).unwrap_err();
        assert!(matches!(err, AtlasLoadError::LucideMismatch { .. }));
        write(LUCIDE_VERSION, ATLAS_VERSION + 1);
        let err = load_baked_manifest_from(&dir).unwrap_err();
        assert!(matches!(err, AtlasLoadError::SchemaVersion { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_dir_is_an_error_not_a_panic() {
        let dir = Path::new("/nonexistent/umber-icon-atlas");
        let err = load_baked_manifest_from(dir).unwrap_err();
        assert!(matches!(err, AtlasLoadError::Io { .. }));
        let ctx = egui::Context::default();
        let err = load_baked_atlas_from(&ctx, IconFamily::Tool, false, dir).unwrap_err();
        assert!(matches!(err, AtlasLoadError::Io { .. }));
    }

    /// The checked-in manifest, if the tool has generated it (it is
    /// pre-seeded by this kill; the tool rewrites it byte-identically).
    /// Absent file = graceful skip, never a failure.
    fn read_repo_manifest() -> Option<BakedManifest> {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons/atlas/manifest.json");
        let Ok(text) = std::fs::read_to_string(path) else {
            return None;
        };
        serde_json::from_str(&text).ok()
    }

    #[test]
    fn baked_manifest_covers_every_declared_id() {
        let Some(manifest) = read_repo_manifest() else {
            eprintln!("skipping: assets/icons/atlas/manifest.json absent");
            return;
        };
        assert_eq!(manifest.lucide, LUCIDE_VERSION);
        // The 2026-10-12 finding, pinned: these two ids were renamed
        // upstream (`fingerprint` -> `fingerprint-pattern`,
        // `flip-horizontal-2` -> `triangles-centerline-dashed-horizontal`)
        // and keep placeholder boxes. If lucide restores them, or renames
        // more, this assertion fails loudly on purpose.
        assert_eq!(
            manifest.missing,
            vec!["fingerprint".to_owned(), "flip-horizontal-2".to_owned()]
        );
        let sections = [
            &manifest.tool,
            &manifest.tool2x,
            &manifest.panel,
            &manifest.panel2x,
        ];
        for name in TOOL_ICONS.iter().chain(PANEL_ICONS.iter()) {
            let rect = sections.iter().find_map(|s| s.icons.get(*name));
            if manifest.missing.iter().any(|m| m == name) {
                assert!(rect.is_none(), "`{name}` is missing but has a rect");
                continue;
            }
            let rect =
                rect.unwrap_or_else(|| panic!("`{name}` has neither a rect nor a missing entry"));
            assert!(rect.w > 0 && rect.h > 0, "`{name}` rect is empty");
        }
        // Uniform cells: every rect equals its section's cell.
        for section in sections {
            for (icon_name, rect) in &section.icons {
                assert_eq!(
                    [rect.w, rect.h],
                    [section.cell, section.cell],
                    "`{icon_name}` is not cell-sized"
                );
            }
        }
    }

    #[test]
    fn baked_manifest_dims_match_packer_math() {
        let Some(manifest) = read_repo_manifest() else {
            eprintln!("skipping: assets/icons/atlas/manifest.json absent");
            return;
        };
        for section in [
            &manifest.tool,
            &manifest.tool2x,
            &manifest.panel,
            &manifest.panel2x,
        ] {
            assert_eq!(
                section.atlas,
                pack_grid(section.order.len(), section.cell).0
            );
            assert_eq!(section.order.len(), section.icons.len());
        }
        // Exact hand-derived layout (pack_grid(8, 20) = 3 cols x 3 rows):
        // the tool must reproduce these numbers, not just any valid grid.
        assert_eq!(manifest.tool.atlas, [60, 60]);
        assert_eq!(manifest.tool2x.atlas, [120, 120]);
        assert_eq!(manifest.panel.atlas, [48, 48]);
        assert_eq!(manifest.panel2x.atlas, [96, 96]);
    }

    /// The checked-in PNGs, once the tool has generated them. Absent files
    /// skip gracefully; present files must decode to the manifest's dims
    /// with real (nonzero-alpha) glyph pixels.
    #[test]
    fn baked_pngs_decode_to_manifest_dims_with_real_pixels() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons/atlas");
        if !dir.join(MANIFEST_FILE).exists() {
            eprintln!("skipping: baked atlas not generated yet");
            return;
        }
        let manifest = load_baked_manifest_from(&dir).expect("manifest loads");
        for (family, hidpi) in [
            (IconFamily::Tool, false),
            (IconFamily::Tool, true),
            (IconFamily::Panel, false),
            (IconFamily::Panel, true),
        ] {
            let file = atlas_file_name(family, hidpi);
            let Ok(bytes) = std::fs::read(dir.join(file)) else {
                eprintln!("skipping: {file} not generated yet");
                continue;
            };
            let (w, h, rgba) = decode_atlas_png(&bytes).expect("png decodes");
            let section = manifest.section(family, hidpi);
            assert_eq!([w, h], section.atlas, "{file} dims");
            let alpha = rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
            assert!(alpha > 0, "{file} is fully transparent");
        }
    }

    #[cfg(feature = "gpu")]
    mod gpu {
        use super::*;

        /// Plain default device; skips gracefully without an adapter (the
        /// `renderer::tests::gpu` convention).
        fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
            let instance = wgpu::Instance::default();
            let adapter = pollster::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
            )
            .ok()?;
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
        }

        #[test]
        fn baked_cells_build_an_egui_texture_and_upload_to_wgpu() {
            let Some((device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping baked_cells_build_an_egui_texture_and_upload_to_wgpu: \
                           no wgpu adapter available"
                );
                return;
            };
            // The egui half: a baked family becomes a stored texture on a
            // headless Context (CPU-side; no adapter needed for the handle).
            let (family, png) = synthetic_family();
            let ctx = egui::Context::default();
            let atlas = assemble_baked_atlas(&ctx, "gpu-test-atlas", &family, &png, "test.png")
                .expect("assembles");
            assert!(atlas.texture().is_some());
            assert!(atlas.uv("a").is_some());
            // The GPU half: the same 4x2 RGBA uploads as Rgba8Unorm (the
            // shapes below mirror umber-gpu's texture_display upload path).
            let extent = wgpu::Extent3d {
                width: 4,
                height: 2,
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("umber_icon_atlas_test"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let rgba = &decode_atlas_png(&png).expect("decodes").2;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * 4),
                    rows_per_image: Some(2),
                },
                extent,
            );
            // A view builds over it (the sampled-image half of the atlas
            // contract); the upload above already panics on any layout
            // or size violation, so reaching here is the assertion.
            let _view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        }
    }
}
