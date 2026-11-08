//! The projection fill's session half (R-A2, the decal projector): row 278
//! of `docs/specs/projection-fill-plan.md`. Owns the RGBA source image and
//! its loader, the [`ProjectionState`] the shell keeps, the frozen camera
//! placement, the target-texel rasterizer [`rasterize_projection`] and the
//! overlay quad. Row 279 (`PaintState::apply_projection`) receives the
//! texels this module builds; rows 280/281 wire the viewport and the panel.
//!
//! The raster is TARGET-TEXEL driven and never raycasts: each texel of the
//! active tile maps to its world point through `umber_mesh::world_at_uv`
//! (row 277), then through the frozen camera's `view_proj` to the frozen
//! viewport, then through `umber_core::stencil::inverse_map` into the
//! image. One O(1) lookup chain per texel, ONE tile-sized RGBA buffer.
//!
//! v1 cut line (plan §7 names the follow-ups):
//!
//! - **No tiling/repeat**: outside the placed image every texel is
//!   transparent (the zero border).
//! - **No live projector**: Apply rasterizes through the placement frozen
//!   at [`ProjectionState::arm`], never the live camera.
//! - **No depth or backface culling**: a texel whose world point sits
//!   behind another surface still writes, and both faces of a sheet
//!   write. Only behind-the-eye (`w <= 0`) and off-viewport (NDC outside
//!   `[-1, 1]`) texels are skipped.
//!
//! Judgment calls (the next kill extends from these):
//!
//! - **Screen space is VIEWPORT-LOCAL px** (origin at the frozen rect's
//!   top-left), not absolute egui px. The brief's formula adds
//!   `rect.left()`/`rect.top()`; this module drops that offset because
//!   every consumer of a [`StencilTransform`] is viewport-local: the
//!   S-chords (`viewport.rs` `StencilChord`), `fit_to_viewport` (centers at
//!   `view / 2`) and `Viewport::screen_quad` (the painter adds `rect.min`).
//!   Row 280 reuses those chords verbatim on [`ProjectionState::transform`],
//!   so the raster must read the same space. [`overlay_quad`] is
//!   viewport-local for the same reason: the drawer adds `rect.min`.
//! - **Bilinear is premultiplied**: the four taps are weighted by their
//!   alpha before the RGB average and un-premultiplied after, so the zero
//!   border (and transparent source texels) fade the alpha without
//!   dragging the edge color toward black. Taps sit at texel centers
//!   (`i + 0.5`), the same convention as `umber_core::stencil`'s sampler.
//! - **An `.svg` source projects as WHITE**: `SvgRaster::to_rgba` is white
//!   RGB + coverage alpha — the svg is a mask SHAPE, not artwork. Its
//!   raster cap is `max_pixels.min(DEFAULT_SVG_RASTER_PIXELS)`.
//! - **The input-byte budget runs from metadata BEFORE the read** for both
//!   formats. SVGs use `MAX_SVG_INPUT_BYTES` (16 MiB, refused as
//!   `SvgError::InputTooLarge`). PNGs use `max(16 MiB, 5 * max_pixels)`
//!   ([`png_byte_cap`]): a flat 16 MiB would refuse a legal 2048² noise PNG
//!   (raw RGBA alone is 16 MiB; 5 B/px covers RGBA + filter bytes + zlib
//!   and chunk overhead). The pixel cap then runs twice like the stencil
//!   loader's: on the IHDR's declared size before decode, and on the
//!   decoded size.
//! - **The PNG decoder is `umber_graph::decode_png_rgba8`** (top-down
//!   rows, the screen's y-down order) — the stencil loader's reason, see
//!   `stencil.rs`'s module docs.
//! - **Default opacity is [`DEFAULT_PROJECTION_OPACITY`] = 0.5**, the
//!   stencil overlay's default (`display_panel::DEFAULT_STENCIL_OPACITY`)
//!   carried over: here one value drives both the overlay preview and the
//!   alpha Apply writes, so the preview shows what lands, and a first
//!   Apply at half strength is never a full overwrite.
//! - **`adopt` disarms**: a new image refits its transform to the view, so
//!   a placement frozen for the previous image no longer describes it.
//!   Row 280 calls [`ProjectionState::arm`] with the live camera + rect
//!   after a successful adopt (the plan's "captured at adopt/arm").
//! - **`disarm` keeps `frozen`**: the chords stop driving the projection,
//!   but the last placement stays, so an Apply is still deterministic;
//!   re-arm overwrites it (plan §6: "Re-arm re-freezes").
//! - **A degenerate frozen rect** (zero/non-finite width or height, so no
//!   aspect) rasterizes to `None`, like a degenerate transform.
//! - **Refusals reuse the stencil's catalog keys** (`overlays.stencil-*`):
//!   their English/pt-BR texts name no stencil ("the file could not be
//!   read.", "… is over the … limit.") and this kill owns no `.ftl`; row
//!   281 may give the projection its own keys.

use fluent::FluentValue;
use std::path::{Path, PathBuf};
use thiserror::Error;
use umber_core::stencil::{fit_to_viewport, forward_map, inverse_map, StencilTransform};
use umber_core::svg::{SvgError, DEFAULT_SVG_RASTER_PIXELS, MAX_SVG_INPUT_BYTES};
use umber_gpu::camera::OrbitCamera;

/// The projection's opacity on a fresh session (overlay preview AND the
/// applied alpha; see the module docs).
pub const DEFAULT_PROJECTION_OPACITY: f32 = 0.5;

/// The default source-image pixel cap (2048², the plan §4 raster cap and
/// the svg rasterizer's own default).
pub const DEFAULT_MAX_PROJECTION_PIXELS: u32 = 2048 * 2048;

/// Everything that can go wrong loading a projection source.
#[derive(Debug, Error)]
pub enum ProjectionLoadError {
    /// The file could not be read (or its metadata could not).
    #[error("cannot read the projection file: {0}")]
    Io(#[from] std::io::Error),
    /// The PNG file is over its input-byte budget ([`png_byte_cap`]),
    /// refused from metadata before the read.
    #[error("projection file is {bytes} bytes, over the {max_bytes}-byte cap")]
    InputTooLarge {
        /// The file length.
        bytes: u64,
        /// The cap it exceeded.
        max_bytes: u64,
    },
    /// Over the pixel cap (declared by the header or decoded).
    #[error("projection image {width}x{height} exceeds the {max}-pixel cap")]
    TooLarge {
        /// The image width.
        width: u32,
        /// The image height.
        height: u32,
        /// The cap it exceeded.
        max: usize,
    },
    /// Not a PNG the decoder accepts (8-bit RGB/RGBA, non-interlaced).
    #[error("projection decode failed: {0}")]
    Png(#[from] umber_graph::ImageError),
    /// The SVG did not parse or rasterize (malformed, no size, over a cap,
    /// or over [`MAX_SVG_INPUT_BYTES`] as `InputTooLarge`).
    #[error("projection SVG rejected: {0}")]
    Svg(#[from] SvgError),
    /// Decoded bytes that do not form a `width * height` RGBA8 image
    /// (a zero dimension or a byte-count mismatch).
    #[error("projection image {width}x{height} has {bytes} bytes")]
    Malformed {
        /// The claimed width.
        width: u32,
        /// The claimed height.
        height: u32,
        /// The byte count received.
        bytes: usize,
    },
}

impl ProjectionLoadError {
    /// The catalog key the panel shows for this refusal (the stencil's
    /// keys; see the module docs).
    pub fn reason_key(&self) -> &'static str {
        match self {
            // An over-budget PNG is refused unread: "the file could not
            // be read." is the honest existing text (the too-large key
            // reads in pixels, which a byte refusal has none of).
            Self::Io(_) | Self::InputTooLarge { .. } => "overlays.stencil-unreadable",
            Self::TooLarge { .. } => "overlays.stencil-too-large",
            Self::Svg(SvgError::TooLarge { .. }) => "overlays.stencil-too-large",
            Self::Svg(_) => "overlays.stencil-unsupported-svg",
            Self::Png(_) | Self::Malformed { .. } => "overlays.stencil-unsupported",
        }
    }

    /// The refusal rendered through the catalogs (the English detail
    /// stays in the log line).
    pub fn localized(&self) -> String {
        let too_large = |w: u32, h: u32, cap: usize| {
            crate::i18n::tr_args(
                "overlays.stencil-too-large",
                [
                    ("width", FluentValue::from(i64::from(w))),
                    ("height", FluentValue::from(i64::from(h))),
                    ("max", FluentValue::from(cap as i64)),
                ],
            )
        };
        match self {
            Self::TooLarge { width, height, max } => too_large(*width, *height, *max),
            Self::Svg(SvgError::TooLarge {
                width,
                height,
                max_pixels,
            }) => too_large(*width, *height, *max_pixels),
            Self::Svg(_) => crate::i18n::tr_args(
                "overlays.stencil-unsupported-svg",
                [("detail", FluentValue::from(self.to_string()))],
            ),
            _ => crate::i18n::tr(self.reason_key()),
        }
    }
}

/// A tightly-packed RGBA8 source image, rows top-down (screen y-down).
/// The fields are private so `rgba.len() == width * height * 4` holds
/// (and both dimensions are nonzero).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl RgbaImage {
    /// Wraps `rgba` as a `width` x `height` image.
    ///
    /// # Errors
    ///
    /// [`ProjectionLoadError::Malformed`] for a zero dimension, an
    /// overflowing size, or a byte count other than `width * height * 4`.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, ProjectionLoadError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4));
        if width == 0 || height == 0 || expected != Some(rgba.len()) {
            return Err(ProjectionLoadError::Malformed {
                width,
                height,
                bytes: rgba.len(),
            });
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// Width in image px.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in image px.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The `width * height * 4` bytes, row-major, top-down.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// The texel at `(x, y)` as f32 channels; all zero outside the image
    /// (the zero border).
    fn texel(&self, x: i64, y: i64) -> [f32; 4] {
        let in_x = (0..i64::from(self.width)).contains(&x);
        let in_y = (0..i64::from(self.height)).contains(&y);
        if !(in_x && in_y) {
            return [0.0; 4];
        }
        // In range on both axes, so both casts are lossless.
        let at = (y as usize * self.width as usize + x as usize) * 4;
        match self.rgba.get(at..at + 4) {
            Some(px) => [
                f32::from(px[0]),
                f32::from(px[1]),
                f32::from(px[2]),
                f32::from(px[3]),
            ],
            None => [0.0; 4],
        }
    }

    /// Bilinear sample at image-px `img_pt` (texel centers at `i + 0.5`),
    /// ZERO border: outside the image the alpha falls to 0. Premultiplied
    /// (see the module docs): a fully transparent footprint returns
    /// `[0, 0, 0, 0]`.
    pub fn sample_bilinear(&self, img_pt: [f32; 2]) -> [u8; 4] {
        let [px, py] = img_pt;
        // Beyond one texel past the bounds every tap is border; this
        // also rejects NaN/inf before the float -> int casts.
        let in_reach = (-1.0..=self.width as f32 + 1.0).contains(&px)
            && (-1.0..=self.height as f32 + 1.0).contains(&py);
        if !in_reach {
            return [0; 4];
        }
        let (ux, uy) = (px - 0.5, py - 0.5);
        let (x0, y0) = (ux.floor(), uy.floor());
        let (fx, fy) = (ux - x0, uy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let taps = [
            (self.texel(x0, y0), (1.0 - fx) * (1.0 - fy)),
            (self.texel(x0 + 1, y0), fx * (1.0 - fy)),
            (self.texel(x0, y0 + 1), (1.0 - fx) * fy),
            (self.texel(x0 + 1, y0 + 1), fx * fy),
        ];
        let mut rgb = [0.0f32; 3];
        let mut alpha = 0.0f32;
        for (t, weight) in taps {
            let wa = weight * t[3];
            alpha += wa;
            for (acc, c) in rgb.iter_mut().zip(t) {
                *acc += c * wa;
            }
        }
        if alpha <= 0.0 {
            return [0; 4];
        }
        // Convex combinations of 0..=255 stay in range; the casts
        // saturate regardless.
        let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
        [
            byte(rgb[0] / alpha),
            byte(rgb[1] / alpha),
            byte(rgb[2] / alpha),
            byte(alpha),
        ]
    }
}

/// The PNG input-byte budget for a `max_pixels` cap:
/// `max(MAX_SVG_INPUT_BYTES, 5 * max_pixels)` (see the module docs).
pub fn png_byte_cap(max_pixels: u32) -> u64 {
    (MAX_SVG_INPUT_BYTES as u64).max(u64::from(max_pixels) * 5)
}

/// Refuses a `width` x `height` image over `max_pixels`.
fn check_size(width: u32, height: u32, max_pixels: u32) -> Result<(), ProjectionLoadError> {
    if u64::from(width) * u64::from(height) <= u64::from(max_pixels) {
        Ok(())
    } else {
        Err(ProjectionLoadError::TooLarge {
            width,
            height,
            max: max_pixels as usize,
        })
    }
}

/// The IHDR's declared `(width, height)` (bytes 16..24 when 12..16 spell
/// `IHDR`); `None` otherwise — the decoder reports those. The stencil
/// loader's private helper, mirrored.
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let be = |at: usize| -> Option<u32> {
        let b = bytes.get(at..at + 4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    Some((be(16)?, be(20)?))
}

/// Builds the RGBA image from PNG `bytes`, refusing anything over
/// `max_pixels` before decoding (the header's declared size) and after
/// (the decoded size). The RGBA is kept, never thresholded.
pub fn projection_from_png_bytes(
    bytes: &[u8],
    max_pixels: u32,
) -> Result<RgbaImage, ProjectionLoadError> {
    if let Some((width, height)) = png_dimensions(bytes) {
        check_size(width, height, max_pixels)?;
    }
    let (width, height, rgba) = umber_graph::decode_png_rgba8(bytes)?;
    check_size(width, height, max_pixels)?;
    RgbaImage::new(width, height, rgba)
}

/// Loads a projection source from `path` under `max_pixels`: an
/// `.svg`/`.SVG` extension is rasterized (white + coverage alpha, cap
/// `max_pixels.min(DEFAULT_SVG_RASTER_PIXELS)`); every other extension is
/// decoded as a PNG with its RGBA kept. Both refuse an oversized file from
/// its metadata BEFORE the read (see the module docs for the budgets).
pub fn load_image_rgba8(path: &Path, max_pixels: u32) -> Result<RgbaImage, ProjectionLoadError> {
    let is_svg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    let meta = std::fs::metadata(path)?;
    if !is_svg {
        let cap = png_byte_cap(max_pixels);
        if meta.len() > cap {
            return Err(ProjectionLoadError::InputTooLarge {
                bytes: meta.len(),
                max_bytes: cap,
            });
        }
        let bytes = std::fs::read(path)?;
        return projection_from_png_bytes(&bytes, max_pixels);
    }
    if meta.len() > MAX_SVG_INPUT_BYTES as u64 {
        return Err(ProjectionLoadError::Svg(SvgError::InputTooLarge {
            bytes: usize::try_from(meta.len()).unwrap_or(usize::MAX),
            max_bytes: MAX_SVG_INPUT_BYTES,
        }));
    }
    let bytes = std::fs::read(path)?;
    let cap = (max_pixels as usize).min(DEFAULT_SVG_RASTER_PIXELS);
    let raster = umber_core::svg::rasterize_svg(&bytes, cap)?;
    RgbaImage::new(raster.width(), raster.height(), raster.to_rgba())
}

/// The camera + viewport rect captured at arm: Apply projects through
/// this, never the live camera (plan §6 "Freeze").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrozenPlacement {
    /// The camera as it was at arm.
    pub camera: OrbitCamera,
    /// The viewport rect at arm (its size gives the aspect and the
    /// viewport-local screen space; its offset is not used by the raster).
    pub rect: egui::Rect,
}

/// The session's projection: the source image, where it sits on the
/// viewport, how strongly it applies, and the frozen placement. Session
/// state, never serialized (v1).
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionState {
    /// The source image (`None` until a load succeeds).
    pub image: Option<RgbaImage>,
    /// Where the image sits in viewport-local px.
    pub transform: StencilTransform,
    /// Overlay preview + applied alpha, `0..=1`.
    pub opacity: f32,
    /// Whether the S-chords drive this transform (row 280).
    pub armed: bool,
    /// The placement the last arm froze.
    pub frozen: Option<FrozenPlacement>,
    /// The file the image came from (the panel's path label).
    pub path: Option<PathBuf>,
    /// The last load's refusal, localized (cleared by the next success).
    pub load_error: Option<String>,
}

impl Default for ProjectionState {
    /// No image, the default transform, unarmed, nothing frozen,
    /// [`DEFAULT_PROJECTION_OPACITY`].
    fn default() -> Self {
        Self {
            image: None,
            transform: StencilTransform::default(),
            opacity: DEFAULT_PROJECTION_OPACITY,
            armed: false,
            frozen: None,
            path: None,
            load_error: None,
        }
    }
}

impl ProjectionState {
    /// Takes a load's outcome: success adopts the image, remembers `path`,
    /// fits it to `view` (`[width, height]`, viewport px) and DISARMS (see
    /// the module docs); a refusal keeps the previous image and records
    /// the reason.
    pub fn adopt(
        &mut self,
        path: PathBuf,
        result: Result<RgbaImage, ProjectionLoadError>,
        view: [f32; 2],
    ) {
        match result {
            Ok(image) => {
                self.transform = fit_to_viewport(image.width(), image.height(), view[0], view[1]);
                self.image = Some(image);
                self.path = Some(path);
                self.load_error = None;
                self.armed = false;
                self.frozen = None;
            }
            Err(err) => {
                log::warn!("projection {} refused: {err}", path.display());
                self.load_error = Some(err.localized());
            }
        }
    }

    /// Freezes `camera` + `rect` as the placement and arms the chords.
    /// Re-arming re-freezes.
    pub fn arm(&mut self, camera: OrbitCamera, rect: egui::Rect) {
        self.frozen = Some(FrozenPlacement { camera, rect });
        self.armed = true;
    }

    /// Stops the chords driving the projection; the frozen placement
    /// stays (see the module docs).
    pub fn disarm(&mut self) {
        self.armed = false;
    }
}

/// Rasterizes the projection into `tile`'s `w` x `h` texels (row-major,
/// top-down, RGBA8, `w * h * 4` bytes) through the frozen placement.
/// `None` without an image, without a frozen placement, under a
/// degenerate transform (scale not finite and positive) or a degenerate
/// frozen rect. Texels off the mesh, behind the eye, off the frozen
/// viewport or outside the image write `[0, 0, 0, 0]`.
pub fn rasterize_projection(
    state: &ProjectionState,
    mesh: &umber_mesh::MeshData,
    tile: u16,
    w: u32,
    h: u32,
) -> Option<Vec<u8>> {
    let image = state.image.as_ref()?;
    let frozen = state.frozen.as_ref()?;
    let scale = state.transform.scale;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let rect = frozen.rect;
    let (rw, rh) = (rect.width(), rect.height());
    if !rw.is_finite() || !rh.is_finite() || rw <= 0.0 || rh <= 0.0 {
        return None;
    }
    let len = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    let mut out = vec![0u8; len];
    let (ox, oy) = umber_mesh::uv_gbuffer::tile_offset(tile);
    let vp = frozen.camera.view_proj(rw / rh);
    let size = [image.width(), image.height()];
    for y in 0..h {
        for x in 0..w {
            let uv = umber_mesh::uv_gbuffer::texel_center_uv(ox, oy, x, y, w, h);
            let Some(world) = umber_mesh::world_at_uv(mesh, uv) else {
                continue;
            };
            let clip = vp * glam::Vec4::new(world[0], world[1], world[2], 1.0);
            // `w <= 0` is at or behind the eye; the divide would mirror it.
            if clip.w.is_nan() || clip.w <= 0.0 {
                continue;
            }
            let ndc = clip.truncate() / clip.w;
            if !(-1.0..=1.0).contains(&ndc.x) || !(-1.0..=1.0).contains(&ndc.y) {
                continue;
            }
            // Viewport-local px (see the module docs), y down.
            let screen = [(ndc.x + 1.0) * 0.5 * rw, (1.0 - ndc.y) * 0.5 * rh];
            let img_pt = inverse_map(&state.transform, size, screen);
            let rgba = image.sample_bilinear(img_pt);
            let at = (y as usize * w as usize + x as usize) * 4;
            if let Some(dst) = out.get_mut(at..at + 4) {
                dst.copy_from_slice(&rgba);
            }
        }
    }
    Some(out)
}

/// The projection quad's corners in viewport-local px (the drawer adds
/// `rect.min`): the image corners `(0,0)`, `(w,0)`, `(w,h)`, `(0,h)`
/// through `forward_map` — exactly `Viewport::screen_quad`'s map, so the
/// quad the eye sees is the region the raster samples.
pub fn overlay_quad(transform: &StencilTransform, image_size: [u32; 2]) -> [egui::Pos2; 4] {
    let (w, h) = (image_size[0] as f32, image_size[1] as f32);
    [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]].map(|corner| {
        let [x, y] = forward_map(transform, image_size, corner);
        egui::pos2(x, y)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("umber-projection-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
        let path = temp_dir().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// An image from per-pixel colors, row-major.
    fn image(w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&px(x, y));
            }
        }
        RgbaImage::new(w, h, rgba).unwrap()
    }

    /// The 2x2 unit quad on z=0 spanning [-1, 1]² with unit UVs
    /// (the viewport tests' fixture: u runs with +x, v with +y).
    fn quad() -> umber_mesh::MeshData {
        umber_mesh::MeshData {
            positions: vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Default::default()
        }
    }

    /// A camera on +z looking straight down -z at `target` (yaw = pitch
    /// = 0 puts the eye at `target + (0, 0, distance)`).
    fn head_on(target: glam::Vec3, distance: f32) -> OrbitCamera {
        OrbitCamera {
            target,
            yaw: 0.0,
            pitch: 0.0,
            distance,
            ..OrbitCamera::default()
        }
    }

    fn view_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0))
    }

    /// `img` centered on the 100x100 view at scale `100 / img.width`,
    /// frozen through `camera`.
    fn armed(img: RgbaImage, camera: OrbitCamera) -> ProjectionState {
        let scale = 100.0 / img.width() as f32;
        let mut state = ProjectionState {
            image: Some(img),
            transform: StencilTransform {
                center: [50.0, 50.0],
                scale,
                rotation: 0.0,
            },
            ..ProjectionState::default()
        };
        state.arm(camera, view_rect());
        state
    }

    fn texel(out: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * w + x) * 4) as usize;
        [out[at], out[at + 1], out[at + 2], out[at + 3]]
    }

    /// Left half red, right half blue, 8x8.
    fn red_blue() -> RgbaImage {
        image(8, 8, |x, _| if x < 4 { RED } else { BLUE })
    }

    // --- loader ---

    #[test]
    fn over_budget_svg_is_refused_from_metadata_before_the_read() {
        let path = temp_dir().join("huge.svg");
        let file = std::fs::File::create(&path).unwrap();
        // Sparse: no 16 MiB actually written.
        file.set_len(MAX_SVG_INPUT_BYTES as u64 + 1).unwrap();
        drop(file);
        // With read permission revoked a read would fail as Io, so the
        // InputTooLarge below proves the refusal came from metadata
        // (under root the read would succeed, and rasterize_svg's own
        // byte check would still refuse — the pin holds, more weakly).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let result = load_image_rgba8(&path, DEFAULT_MAX_PROJECTION_PIXELS);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        let _ = std::fs::remove_file(&path);
        match result {
            Err(ProjectionLoadError::Svg(SvgError::InputTooLarge { bytes, max_bytes })) => {
                assert_eq!(bytes, MAX_SVG_INPUT_BYTES + 1);
                assert_eq!(max_bytes, MAX_SVG_INPUT_BYTES);
            }
            other => panic!("expected InputTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn png_byte_budget_is_the_larger_of_16_mib_and_5_bytes_per_pixel() {
        assert_eq!(png_byte_cap(16), 16 * 1024 * 1024);
        assert_eq!(png_byte_cap(4096 * 4096), 5 * 4096 * 4096);
    }

    #[test]
    fn over_cap_png_is_refused_by_name() {
        let raw = image(4, 4, |_, _| RED);
        let png = umber_graph::encode_png_rgba8(4, 4, raw.rgba()).unwrap();
        let path = write_temp("over-cap.png", &png);
        match load_image_rgba8(&path, 15) {
            Err(ProjectionLoadError::TooLarge { width, height, max }) => {
                assert_eq!((width, height, max), (4, 4, 15));
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
        // Exactly at the cap loads.
        assert!(load_image_rgba8(&path, 16).is_ok());
    }

    #[test]
    fn small_png_round_trips_its_rgba_unthresholded() {
        // Alpha 100 would threshold to 0 in the stencil loader: kept here.
        let raw = image(2, 2, |x, y| [x as u8 * 200, y as u8 * 90, 7, 100 + x as u8]);
        let png = umber_graph::encode_png_rgba8(2, 2, raw.rgba()).unwrap();
        let path = write_temp("small.png", &png);
        let loaded = load_image_rgba8(&path, DEFAULT_MAX_PROJECTION_PIXELS).unwrap();
        assert_eq!(loaded, raw);
    }

    #[test]
    fn small_svg_loads_as_white_plus_coverage_alpha() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect x="0" y="0" width="4" height="8"/></svg>"#;
        let path = write_temp("half.SVG", svg.as_bytes());
        let loaded = load_image_rgba8(&path, DEFAULT_MAX_PROJECTION_PIXELS).unwrap();
        let raster = umber_core::svg::rasterize_svg_default(svg.as_bytes()).unwrap();
        assert_eq!((loaded.width(), loaded.height()), (8, 8));
        assert_eq!(loaded.rgba(), raster.to_rgba().as_slice());
        // Inside the rect: white, opaque; outside: white, clear.
        assert_eq!(&loaded.rgba()[0..4], &[255, 255, 255, 255]);
        assert_eq!(&loaded.rgba()[7 * 4..8 * 4], &[255, 255, 255, 0]);
    }

    #[test]
    fn bad_buffers_are_malformed() {
        assert!(matches!(
            RgbaImage::new(2, 2, vec![0; 15]),
            Err(ProjectionLoadError::Malformed { bytes: 15, .. })
        ));
        assert!(matches!(
            RgbaImage::new(0, 2, vec![]),
            Err(ProjectionLoadError::Malformed { width: 0, .. })
        ));
    }

    // --- sampler ---

    #[test]
    fn bilinear_hits_texel_centers_and_fades_into_the_zero_border() {
        let img = image(2, 1, |x, _| if x == 0 { RED } else { BLUE });
        assert_eq!(img.sample_bilinear([0.5, 0.5]), RED);
        assert_eq!(img.sample_bilinear([1.5, 0.5]), BLUE);
        // Midway: 50/50 RGB, full alpha.
        assert_eq!(img.sample_bilinear([1.0, 0.5]), [128, 0, 128, 255]);
        // The left edge (x = 0): half the footprint is border, so alpha
        // halves while the color stays red (premultiplied).
        assert_eq!(img.sample_bilinear([0.0, 0.5]), [255, 0, 0, 128]);
        assert_eq!(img.sample_bilinear([-5.0, 0.5]), CLEAR);
        assert_eq!(img.sample_bilinear([f32::NAN, 0.5]), CLEAR);
    }

    // --- raster ---

    #[test]
    fn center_texel_samples_the_source_center() {
        let img = image(3, 3, |x, y| if (x, y) == (1, 1) { GREEN } else { BLACK });
        let state = armed(img, head_on(glam::Vec3::ZERO, 5.0));
        // 9x9: texel (4, 4)'s center is uv (0.5, 0.5) = world origin,
        // which projects to the view center = image px (1.5, 1.5).
        let out = rasterize_projection(&state, &quad(), 1001, 9, 9).unwrap();
        assert_eq!(out.len(), 9 * 9 * 4);
        assert_eq!(texel(&out, 9, 4, 4), GREEN);
    }

    #[test]
    fn red_left_half_lands_on_the_left_texels() {
        let state = armed(red_blue(), head_on(glam::Vec3::ZERO, 5.0));
        let out = rasterize_projection(&state, &quad(), 1001, 8, 8).unwrap();
        // The quad covers ~screen 25.9..74.1 (distance 5, fov 45°), so
        // texels 0..=2 sample image x in ~2.3..3.3 (red taps only) and
        // texels 5..=7 ~4.7..5.7 (blue taps only).
        for y in 0..8 {
            for x in 0..3 {
                assert_eq!(texel(&out, 8, x, y), RED, "texel ({x}, {y})");
            }
            for x in 5..8 {
                assert_eq!(texel(&out, 8, x, y), BLUE, "texel ({x}, {y})");
            }
        }
    }

    #[test]
    fn a_quarter_turn_moves_red_to_the_top_rows() {
        let mut state = armed(red_blue(), head_on(glam::Vec3::ZERO, 5.0));
        state.transform.rotation = FRAC_PI_2;
        let out = rasterize_projection(&state, &quad(), 1001, 8, 8).unwrap();
        // Clockwise quarter turn: the image's left (red) half now sits
        // ABOVE the center on the y-down screen = world +y = the top
        // texel rows (v near 1).
        for x in 0..8 {
            for y in 0..3 {
                assert_eq!(texel(&out, 8, x, y), RED, "texel ({x}, {y})");
            }
            for y in 5..8 {
                assert_eq!(texel(&out, 8, x, y), BLUE, "texel ({x}, {y})");
            }
        }
    }

    #[test]
    fn texels_projecting_outside_the_image_are_clear() {
        let mut state = armed(image(2, 2, |_, _| RED), head_on(glam::Vec3::ZERO, 5.0));
        // A 10 px image square at the view center (45..55).
        state.transform.scale = 5.0;
        let out = rasterize_projection(&state, &quad(), 1001, 8, 8).unwrap();
        // Corner texel: world (-0.875, 0.875) -> screen ~(28.9, 28.9),
        // far outside the image.
        assert_eq!(texel(&out, 8, 0, 0), CLEAR);
        assert_eq!(texel(&out, 8, 7, 7), CLEAR);
    }

    #[test]
    fn texels_off_the_frozen_viewport_are_clear() {
        // Distance 1: the view half-height at the plane is tan(22.5°) ≈
        // 0.414, so the quad's corners (±0.875 texel centers) fall
        // outside NDC even though the image covers the whole view.
        let state = armed(image(2, 2, |_, _| RED), head_on(glam::Vec3::ZERO, 1.0));
        let out = rasterize_projection(&state, &quad(), 1001, 8, 8).unwrap();
        assert_eq!(texel(&out, 8, 0, 0), CLEAR);
        // The center texels stay inside the view.
        assert_eq!(texel(&out, 8, 4, 4), RED);
    }

    #[test]
    fn camera_behind_the_plane_writes_nothing() {
        // Eye at z = -5 looking toward -z: the z = 0 quad is behind it
        // (clip w < 0). Without the w test, the world origin's x/w = 0
        // would land inside NDC and sample the opaque image.
        let mut state = armed(
            image(2, 2, |_, _| RED),
            head_on(glam::Vec3::new(0.0, 0.0, -10.0), 5.0),
        );
        state.transform.scale = 1000.0;
        let out = rasterize_projection(&state, &quad(), 1001, 8, 8).unwrap();
        assert!(out.iter().all(|&b| b == 0));
    }

    #[test]
    fn texels_off_the_tile_are_clear() {
        // Tile 1002 holds no triangle of the unit quad.
        let state = armed(image(2, 2, |_, _| RED), head_on(glam::Vec3::ZERO, 5.0));
        let out = rasterize_projection(&state, &quad(), 1002, 4, 4).unwrap();
        assert_eq!(out, vec![0; 4 * 4 * 4]);
    }

    #[test]
    fn no_frozen_placement_or_degenerate_inputs_are_none() {
        let mut state = armed(image(2, 2, |_, _| RED), head_on(glam::Vec3::ZERO, 5.0));
        let mesh = quad();
        assert!(rasterize_projection(&state, &mesh, 1001, 4, 4).is_some());

        let mut unfrozen = state.clone();
        unfrozen.frozen = None;
        assert!(rasterize_projection(&unfrozen, &mesh, 1001, 4, 4).is_none());

        let mut no_image = state.clone();
        no_image.image = None;
        assert!(rasterize_projection(&no_image, &mesh, 1001, 4, 4).is_none());

        let mut flat_rect = state.clone();
        flat_rect.arm(
            head_on(glam::Vec3::ZERO, 5.0),
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 0.0)),
        );
        assert!(rasterize_projection(&flat_rect, &mesh, 1001, 4, 4).is_none());

        state.transform.scale = 0.0;
        assert!(rasterize_projection(&state, &mesh, 1001, 4, 4).is_none());
        state.transform.scale = f32::NAN;
        assert!(rasterize_projection(&state, &mesh, 1001, 4, 4).is_none());
    }

    // --- state ---

    #[test]
    fn default_state_is_empty_unarmed_at_half_opacity() {
        let s = ProjectionState::default();
        assert_eq!(s.opacity, 0.5);
        assert!(!s.armed && s.frozen.is_none());
        assert!(s.image.is_none() && s.path.is_none() && s.load_error.is_none());
        assert_eq!(s.transform, StencilTransform::default());
    }

    #[test]
    fn arm_freezes_disarm_keeps_the_freeze_and_adopt_refits_and_disarms() {
        let mut s = ProjectionState::default();
        let cam = head_on(glam::Vec3::ZERO, 5.0);
        s.arm(cam, view_rect());
        assert!(s.armed);
        assert_eq!(
            s.frozen,
            Some(FrozenPlacement {
                camera: cam,
                rect: view_rect()
            })
        );
        s.disarm();
        assert!(!s.armed && s.frozen.is_some());

        s.arm(cam, view_rect());
        s.adopt(
            PathBuf::from("a.png"),
            Ok(image(4, 2, |_, _| RED)),
            [200.0, 100.0],
        );
        // fit_to_viewport: min(200/4, 100/2) = 50, centered.
        assert_eq!(s.transform.scale, 50.0);
        assert_eq!(s.transform.center, [100.0, 50.0]);
        assert!(!s.armed && s.frozen.is_none());
        assert_eq!(s.path.as_deref(), Some(Path::new("a.png")));

        s.adopt(
            PathBuf::from("b.png"),
            Err(ProjectionLoadError::TooLarge {
                width: 9,
                height: 9,
                max: 4,
            }),
            [200.0, 100.0],
        );
        // A refusal keeps the image and path, and records a reason.
        assert_eq!(s.path.as_deref(), Some(Path::new("a.png")));
        assert!(s.image.is_some() && s.load_error.is_some());
    }

    #[test]
    fn overlay_quad_maps_the_corners_through_forward_map() {
        let t = StencilTransform {
            center: [100.0, 50.0],
            scale: 2.0,
            rotation: 0.0,
        };
        let quad = overlay_quad(&t, [40, 20]);
        // Top-left: (0 - 20) * 2 + 100 = 60, (0 - 10) * 2 + 50 = 30.
        assert_eq!(quad[0], egui::pos2(60.0, 30.0));
        assert_eq!(quad[2], egui::pos2(140.0, 70.0));
        let f = forward_map(&t, [40, 20], [40.0, 0.0]);
        assert_eq!(quad[1], egui::pos2(f[0], f[1]));
    }
}
