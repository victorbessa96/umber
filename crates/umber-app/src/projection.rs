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
//! - **Tiling is opt-in (v2a, row 287,
//!   `docs/loop/briefs/2026-11-08-projection-tiling-design.md`)**: with
//!   [`ProjectionState::tiling`] off, outside the placed image every
//!   texel is transparent (the zero border), byte-identical to v1. With
//!   it on, the per-texel image point wraps `rem_euclid` into
//!   `[0, w) x [0, h)` and [`RgbaImage::sample_bilinear_wrapped`] wraps
//!   the taps too, so the decal repeats seamlessly (edges blend with the
//!   opposite edge). Every upstream skip (off-mesh, behind the eye,
//!   off-NDC) is unchanged, so tiling never leaks outside the frozen
//!   viewport. v2a cuts: infinite wrap only (no repeat count), no
//!   mirror tiling, no UV-space tiling, and the OVERLAY still previews
//!   one tile (a single quad) while Apply tiles everywhere in view.
//!   The flag is a placement mode, not image data: `adopt` keeps it.
//!   The wrapped sampler clears past [`MAX_WRAPPED_REACH`] (`2^24` px),
//!   where f32 can no longer address texels; the raster always wraps
//!   first, so it never reaches that guard with a finite point.
//! - **The live projector is opt-in (v2b, row 288,
//!   `docs/loop/briefs/2026-11-08-projection-follow-design.md`)**: with
//!   [`ProjectionState::follow_camera`] off, Apply rasterizes through
//!   the placement frozen at [`ProjectionState::arm`], byte-identical to
//!   v1. With it on, `arm` arms the chords but freezes NOTHING
//!   (`frozen = None`), and Apply runs [`rasterize_projection_live`]
//!   through the LIVE camera + live view rect (paint what you see now).
//!   Both entries share one per-texel core (`rasterize_with`), so the
//!   live raster under the frozen camera equals the frozen raster byte
//!   for byte. [`rasterize_projection`] keeps its v1 contract: no frozen
//!   placement, no raster (`None`), so a follow-on caller must use the
//!   live entry. Tiling composes orthogonally (it wraps the image point;
//!   follow picks the camera). The flag is a placement mode: `adopt`,
//!   `disarm` and tiling never touch it. v2b cuts: Apply re-rasterizes
//!   once per click, never per frame or mid-stroke; no persistence.
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

/// The wrapped sampler's reach, `2^24` image px on either axis: past it
/// an f32 coordinate can no longer address single texels (the spacing
/// between representable values reaches 2 px), so a sample there is
/// noise, not a tile. [`RgbaImage::sample_bilinear_wrapped`] returns
/// clear beyond it (the "negative-huge" guard; see the module docs).
pub const MAX_WRAPPED_REACH: f32 = 16_777_216.0;

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
        Self::bilinear(img_pt, |x, y| self.texel(x, y))
    }

    /// Bilinear sample at image-px `img_pt` with WRAPPED taps (R-A2 v2a
    /// tiling): every tap index wraps with `rem_euclid` into the image,
    /// so an edge footprint blends with the OPPOSITE edge (seamless
    /// repeat, never clamp, never the zero border). Same premultiplied
    /// core as [`Self::sample_bilinear`], so wherever no tap leaves the
    /// image the two agree exactly. A non-finite `img_pt`, or one beyond
    /// [`MAX_WRAPPED_REACH`] on either axis, returns `[0, 0, 0, 0]`.
    pub fn sample_bilinear_wrapped(&self, img_pt: [f32; 2]) -> [u8; 4] {
        let [px, py] = img_pt;
        // Rejects NaN/inf (and f32 coordinates too coarse to address a
        // texel) before the float -> int casts.
        let in_reach = (-MAX_WRAPPED_REACH..=MAX_WRAPPED_REACH).contains(&px)
            && (-MAX_WRAPPED_REACH..=MAX_WRAPPED_REACH).contains(&py);
        if !in_reach {
            return [0; 4];
        }
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        Self::bilinear(img_pt, |x, y| self.texel(x.rem_euclid(w), y.rem_euclid(h)))
    }

    /// The shared premultiplied bilinear core over `tap` (texel centers
    /// at `i + 0.5`). The caller has already guarded `img_pt` finite and
    /// in reach.
    fn bilinear(img_pt: [f32; 2], tap: impl Fn(i64, i64) -> [f32; 4]) -> [u8; 4] {
        let [px, py] = img_pt;
        let (ux, uy) = (px - 0.5, py - 0.5);
        let (x0, y0) = (ux.floor(), uy.floor());
        let (fx, fy) = (ux - x0, uy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let taps = [
            (tap(x0, y0), (1.0 - fx) * (1.0 - fy)),
            (tap(x0 + 1, y0), fx * (1.0 - fy)),
            (tap(x0, y0 + 1), (1.0 - fx) * fy),
            (tap(x0 + 1, y0 + 1), fx * fy),
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
    /// Tile (R-A2 v2a): the image repeats infinitely over the frozen
    /// viewport instead of falling to the zero border. A placement mode,
    /// so [`ProjectionState::adopt`] keeps it.
    pub tiling: bool,
    /// Follow camera (R-A2 v2b): Apply projects through the LIVE camera
    /// and view rect instead of a frozen placement, and `arm` freezes
    /// nothing. A placement mode, so [`ProjectionState::adopt`] keeps it.
    pub follow_camera: bool,
}

impl Default for ProjectionState {
    /// No image, the default transform, unarmed, nothing frozen,
    /// [`DEFAULT_PROJECTION_OPACITY`], tiling and follow-camera off.
    fn default() -> Self {
        Self {
            image: None,
            transform: StencilTransform::default(),
            opacity: DEFAULT_PROJECTION_OPACITY,
            armed: false,
            frozen: None,
            path: None,
            load_error: None,
            tiling: false,
            follow_camera: false,
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

    /// Arms the chords. With [`Self::follow_camera`] off it also freezes
    /// `camera` + `rect` as the placement (re-arming re-freezes); with it
    /// on nothing is frozen (`frozen = None`): Apply reads the live
    /// camera instead (see the module docs).
    pub fn arm(&mut self, camera: OrbitCamera, rect: egui::Rect) {
        self.frozen = if self.follow_camera {
            None
        } else {
            Some(FrozenPlacement { camera, rect })
        };
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
/// viewport or (tiling off) outside the image write `[0, 0, 0, 0]`; with
/// [`ProjectionState::tiling`] the image repeats instead (module docs).
///
/// This is the FROZEN entry regardless of [`ProjectionState::follow_camera`]:
/// with follow on, `arm` freezes nothing, so this returns `None` and the
/// caller must use [`rasterize_projection_live`].
pub fn rasterize_projection(
    state: &ProjectionState,
    mesh: &umber_mesh::MeshData,
    tile: u16,
    w: u32,
    h: u32,
) -> Option<Vec<u8>> {
    let frozen = state.frozen.as_ref()?;
    rasterize_with(state, mesh, tile, w, h, &frozen.camera, frozen.rect)
}

/// [`rasterize_projection`] through a LIVE `camera` + viewport `rect`
/// (R-A2 v2b follow camera) instead of the frozen placement; ignores
/// [`ProjectionState::frozen`]. Same output contract and refusals (no
/// image, degenerate transform, degenerate `rect`), same per-texel core.
pub fn rasterize_projection_live(
    state: &ProjectionState,
    mesh: &umber_mesh::MeshData,
    tile: u16,
    w: u32,
    h: u32,
    camera: &OrbitCamera,
    rect: egui::Rect,
) -> Option<Vec<u8>> {
    rasterize_with(state, mesh, tile, w, h, camera, rect)
}

/// The shared per-texel core: `state`'s image + transform + tiling
/// projected through `camera` onto the viewport `rect` (only its size is
/// read; screen space is viewport-local, see the module docs).
fn rasterize_with(
    state: &ProjectionState,
    mesh: &umber_mesh::MeshData,
    tile: u16,
    w: u32,
    h: u32,
    camera: &OrbitCamera,
    rect: egui::Rect,
) -> Option<Vec<u8>> {
    let image = state.image.as_ref()?;
    let scale = state.transform.scale;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let (rw, rh) = (rect.width(), rect.height());
    if !rw.is_finite() || !rh.is_finite() || rw <= 0.0 || rh <= 0.0 {
        return None;
    }
    let len = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    let mut out = vec![0u8; len];
    let (ox, oy) = umber_mesh::uv_gbuffer::tile_offset(tile);
    let vp = camera.view_proj(rw / rh);
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
            let rgba = if state.tiling {
                // Wrap into [0, w) x [0, h) first (NaN/inf stay NaN and
                // the sampler's guard clears them), then wrap the taps.
                let wrapped = [
                    img_pt[0].rem_euclid(size[0] as f32),
                    img_pt[1].rem_euclid(size[1] as f32),
                ];
                image.sample_bilinear_wrapped(wrapped)
            } else {
                image.sample_bilinear(img_pt)
            };
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

    // --- wrapped sampler (R-A2 v2a tiling) ---

    #[test]
    fn wrapped_sampler_clears_non_finite_and_huge_points_and_agrees_in_range() {
        let img = image(4, 4, |x, y| [x as u8 * 60, y as u8 * 50 + 5, 7, 255]);
        for pt in [
            [f32::NAN, 1.5],
            [1.5, f32::NAN],
            [f32::INFINITY, 1.5],
            [1.5, f32::NEG_INFINITY],
            [-1.0e30, 1.5],
            [1.5, -1.0e30],
            [-MAX_WRAPPED_REACH * 2.0, 1.5],
        ] {
            assert_eq!(img.sample_bilinear_wrapped(pt), CLEAR, "{pt:?}");
        }
        // Wherever no tap leaves the image the two samplers agree exactly.
        for pt in [[0.5, 0.5], [1.5, 2.5], [2.0, 1.25], [3.5, 3.5], [1.75, 0.5]] {
            assert_eq!(
                img.sample_bilinear_wrapped(pt),
                img.sample_bilinear(pt),
                "{pt:?}"
            );
        }
        // Texel (2, 1) center, exact: [2 * 60, 1 * 50 + 5, 7, 255].
        assert_eq!(img.sample_bilinear_wrapped([2.5, 1.5]), [120, 55, 7, 255]);
    }

    #[test]
    fn wrapped_sampler_seam_blends_the_opposite_edge() {
        // Column 7 is blue, column 0 is red: across the wrap seam both
        // x = 8 - 0.001 and x = -0.001 sit at ux = 7.499 / -0.501, so
        // the taps are texel 7 (weight 0.501) and texel 0 (0.499):
        // r = 255 * 0.499 = 127.2 -> 127, b = 255 * 0.501 = 127.8 -> 128.
        let img = red_blue();
        let seam = [127, 0, 128, 255];
        assert_eq!(img.sample_bilinear_wrapped([8.0 - 0.001, 4.5]), seam);
        assert_eq!(img.sample_bilinear_wrapped([-0.001, 4.5]), seam);
        // Not clamp (pure blue / pure red at the edges) and not the zero
        // border (half alpha): the v1 sampler at the same points.
        assert_eq!(img.sample_bilinear([8.0 - 0.001, 4.5]), [0, 0, 255, 128]);
        // (Red's weight is 0.499 there: alpha 127.2 -> 127.)
        assert_eq!(img.sample_bilinear([-0.001, 4.5]), [255, 0, 0, 127]);
        // The y seam wraps the same way (rows 7 and 0, red column).
        let rows = image(2, 8, |_, y| if y == 0 { RED } else { BLUE });
        assert_eq!(rows.sample_bilinear_wrapped([0.5, -0.001]), seam);
    }

    // --- raster ---

    /// The v1 chain by hand (world_at_uv -> frozen view_proj -> NDC ->
    /// viewport px -> inverse_map -> zero-border sampler), the reference
    /// tiling-off output must match byte for byte.
    fn v1_reference(state: &ProjectionState, mesh: &umber_mesh::MeshData, n: u32) -> Vec<u8> {
        let frozen = state.frozen.unwrap();
        reference_through(state, mesh, n, &frozen.camera, frozen.rect)
    }

    /// The same hand chain through an explicit `camera` + `rect` (the
    /// follow-camera pins' re-projection reference).
    fn reference_through(
        state: &ProjectionState,
        mesh: &umber_mesh::MeshData,
        n: u32,
        camera: &OrbitCamera,
        rect: egui::Rect,
    ) -> Vec<u8> {
        let image = state.image.as_ref().unwrap();
        let (rw, rh) = (rect.width(), rect.height());
        let vp = camera.view_proj(rw / rh);
        let (ox, oy) = umber_mesh::uv_gbuffer::tile_offset(1001);
        let mut out = vec![0u8; (n * n * 4) as usize];
        for y in 0..n {
            for x in 0..n {
                let uv = umber_mesh::uv_gbuffer::texel_center_uv(ox, oy, x, y, n, n);
                let Some(world) = umber_mesh::world_at_uv(mesh, uv) else {
                    continue;
                };
                let clip = vp * glam::Vec4::new(world[0], world[1], world[2], 1.0);
                if clip.w <= 0.0 {
                    continue;
                }
                let ndc = clip.truncate() / clip.w;
                if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
                    continue;
                }
                let screen = [(ndc.x + 1.0) * 0.5 * rw, (1.0 - ndc.y) * 0.5 * rh];
                let size = [image.width(), image.height()];
                let img_pt = inverse_map(&state.transform, size, screen);
                let at = ((y * n + x) * 4) as usize;
                out[at..at + 4].copy_from_slice(&image.sample_bilinear(img_pt));
            }
        }
        out
    }

    #[test]
    fn tiling_off_is_byte_identical_to_the_v1_chain() {
        let mesh = quad();
        // A small, rotated, off-center image so the zero border, the
        // image interior and the bilinear edges all appear in the tile.
        let mut state = armed(red_blue(), head_on(glam::Vec3::ZERO, 5.0));
        state.transform = StencilTransform {
            center: [43.0, 57.0],
            scale: 3.0,
            rotation: 0.3,
        };
        assert!(!state.tiling);
        let out = rasterize_projection(&state, &mesh, 1001, 16, 16).unwrap();
        assert_eq!(out, v1_reference(&state, &mesh, 16));
        // Sanity: the fixture really exercises both border and image.
        assert!(out.chunks(4).any(|t| t == CLEAR));
        assert!(out.chunks(4).any(|t| t[3] == 255));
    }

    #[test]
    fn tiling_wraps_a_point_left_of_the_image_onto_its_repeat() {
        // 8x8 gradient at scale 12.5, center (156.25, 56.25): texel (4, 4)
        // of a 9x9 tile is the world origin = screen (50, 50), so
        // img_pt = ((50 - 156.25) / 12.5 + 4, (50 - 56.25) / 12.5 + 4)
        // = (-4.5, 3.5), in [-w, 0). Wrapped: (3.5, 3.5) = texel (3, 3).
        let img = image(8, 8, |x, y| [x as u8 * 30, y as u8 * 20 + 5, 7, 255]);
        let mut state = armed(img.clone(), head_on(glam::Vec3::ZERO, 5.0));
        state.transform.center = [156.25, 56.25];
        let off = rasterize_projection(&state, &quad(), 1001, 9, 9).unwrap();
        // v1: -4.5 is beyond the zero border's reach.
        assert_eq!(texel(&off, 9, 4, 4), CLEAR);
        state.tiling = true;
        let on = rasterize_projection(&state, &quad(), 1001, 9, 9).unwrap();
        assert_eq!(texel(&on, 9, 4, 4), img.sample_bilinear([-4.5 + 8.0, 3.5]));
        assert_eq!(texel(&on, 9, 4, 4), [90, 65, 7, 255]);
    }

    #[test]
    fn tiling_fills_outside_the_image_but_never_outside_the_frozen_view() {
        // A 10 px square at the view center: v1 leaves the corners clear,
        // tiling repeats the (uniform red) image over them.
        let mut state = armed(image(2, 2, |_, _| RED), head_on(glam::Vec3::ZERO, 5.0));
        state.transform.scale = 5.0;
        state.tiling = true;
        let out = rasterize_projection(&state, &quad(), 1001, 8, 8).unwrap();
        assert_eq!(texel(&out, 8, 0, 0), RED);
        assert_eq!(texel(&out, 8, 7, 7), RED);
        // Distance 1: the quad's corner texels fall off the frozen NDC,
        // so tiling still leaves them clear; the center stays red.
        let mut state = armed(image(2, 2, |_, _| RED), head_on(glam::Vec3::ZERO, 1.0));
        state.transform.scale = 5.0;
        state.tiling = true;
        let out = rasterize_projection(&state, &quad(), 1001, 8, 8).unwrap();
        assert_eq!(texel(&out, 8, 0, 0), CLEAR);
        assert_eq!(texel(&out, 8, 7, 0), CLEAR);
        assert_eq!(texel(&out, 8, 4, 4), RED);
    }

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

    // --- follow camera (R-A2 v2b) ---

    /// The camera head-on at the origin, panned +x by 0.5 world units
    /// (the "camera moved" fixture of the follow pins).
    fn panned() -> OrbitCamera {
        head_on(glam::Vec3::new(0.5, 0.0, 0.0), 5.0)
    }

    #[test]
    fn follow_off_arm_freezes_and_rasterizes_byte_identical_to_v1() {
        let mut state = armed(red_blue(), head_on(glam::Vec3::ZERO, 5.0));
        assert!(!state.follow_camera);
        assert!(state.armed && state.frozen.is_some());
        state.transform = StencilTransform {
            center: [43.0, 57.0],
            scale: 3.0,
            rotation: 0.3,
        };
        let out = rasterize_projection(&state, &quad(), 1001, 16, 16).unwrap();
        assert_eq!(out, v1_reference(&state, &quad(), 16));
        // Re-arming with follow off re-freezes the new camera.
        state.arm(panned(), view_rect());
        assert_eq!(state.frozen.map(|f| f.camera), Some(panned()));
    }

    #[test]
    fn follow_on_arm_freezes_nothing_and_the_frozen_entry_refuses() {
        let mut state = ProjectionState {
            follow_camera: true,
            ..armed(red_blue(), head_on(glam::Vec3::ZERO, 5.0))
        };
        // `armed()` armed with follow off, so a placement is frozen;
        // re-arming with follow on clears it.
        assert!(state.frozen.is_some());
        let cam = head_on(glam::Vec3::ZERO, 5.0);
        state.arm(cam, view_rect());
        assert!(state.armed);
        assert_eq!(state.frozen, None);
        // The v1 entry needs a frozen placement: callers must go live.
        assert!(rasterize_projection(&state, &quad(), 1001, 8, 8).is_none());
        assert!(
            rasterize_projection_live(&state, &quad(), 1001, 8, 8, &cam, view_rect()).is_some()
        );
        // The live entry keeps the degenerate-rect refusal.
        let flat = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(0.0, 100.0));
        assert!(rasterize_projection_live(&state, &quad(), 1001, 8, 8, &cam, flat).is_none());
    }

    #[test]
    fn live_raster_under_the_frozen_camera_equals_the_frozen_raster() {
        let cam = head_on(glam::Vec3::ZERO, 5.0);
        let mut state = armed(red_blue(), cam);
        state.transform = StencilTransform {
            center: [43.0, 57.0],
            scale: 3.0,
            rotation: 0.3,
        };
        let frozen = rasterize_projection(&state, &quad(), 1001, 16, 16).unwrap();
        let live =
            rasterize_projection_live(&state, &quad(), 1001, 16, 16, &cam, view_rect()).unwrap();
        assert_eq!(live, frozen);
    }

    #[test]
    fn live_raster_reprojects_through_a_moved_camera() {
        // 3x3, green center texel, black ring, at scale 100/3 centered.
        let img = image(3, 3, |x, y| if (x, y) == (1, 1) { GREEN } else { BLACK });
        let state = armed(img, head_on(glam::Vec3::ZERO, 5.0));
        let frozen = rasterize_projection(&state, &quad(), 1001, 9, 9).unwrap();
        assert_eq!(texel(&frozen, 9, 4, 4), GREEN);
        let cam = panned();
        let live =
            rasterize_projection_live(&state, &quad(), 1001, 9, 9, &cam, view_rect()).unwrap();
        assert_ne!(live, frozen, "the moved camera changes the raster");
        assert_eq!(
            live,
            reference_through(&state, &quad(), 9, &cam, view_rect())
        );
        // By hand: texel (4, 4) is the world origin, 0.5 left of the
        // panned eye at depth 5: ndc.x = -0.5 / (5 tan 22.5°) = -0.241421
        // -> screen x = 50 (1 - 0.241421) = 37.9289 -> image x =
        // (37.9289 - 50) / (100/3) + 1.5 = 1.137868 (y stays 1.5). Taps
        // texel 0 (black, 0.362132) and texel 1 (green, 0.637868):
        // g = 255 * 0.637868 = 162.66 -> 163.
        assert_eq!(texel(&live, 9, 4, 4), [0, 163, 0, 255]);
    }

    #[test]
    fn tiling_composes_with_follow_under_the_live_camera() {
        // The v2a wrap fixture (8x8 gradient, scale 12.5, center
        // (156.25, 56.25)), follow on, raster through the PANNED camera:
        // screen x = 37.9289 (see the moved-camera pin) -> image x =
        // (37.9289 - 156.25) / 12.5 + 4 = -5.46569, wrapped +8 = 2.53431:
        // taps 2 (r 60, weight 0.96569) and 3 (r 90, 0.03431): r = 61.03
        // -> 61; y = 3.5 is row 3 exactly: g = 3 * 20 + 5 = 65.
        let img = image(8, 8, |x, y| [x as u8 * 30, y as u8 * 20 + 5, 7, 255]);
        let mut state = ProjectionState {
            follow_camera: true,
            ..armed(img, head_on(glam::Vec3::ZERO, 5.0))
        };
        state.transform.center = [156.25, 56.25];
        state.arm(panned(), view_rect());
        assert_eq!(state.frozen, None);
        let live = |s: &ProjectionState| {
            rasterize_projection_live(s, &quad(), 1001, 9, 9, &panned(), view_rect()).unwrap()
        };
        // Untiled: -5.47 is beyond the zero border's reach.
        assert_eq!(texel(&live(&state), 9, 4, 4), CLEAR);
        state.tiling = true;
        let on = live(&state);
        assert_eq!(texel(&on, 9, 4, 4), [61, 65, 7, 255]);
        // Same bytes as the frozen tiled raster through that camera.
        let mut frozen = state.clone();
        frozen.follow_camera = false;
        frozen.arm(panned(), view_rect());
        assert_eq!(
            rasterize_projection(&frozen, &quad(), 1001, 9, 9).unwrap(),
            on
        );
    }

    // --- state ---

    #[test]
    fn default_state_is_empty_unarmed_at_half_opacity() {
        let s = ProjectionState::default();
        assert_eq!(s.opacity, 0.5);
        assert!(!s.armed && s.frozen.is_none());
        assert!(s.image.is_none() && s.path.is_none() && s.load_error.is_none());
        assert_eq!(s.transform, StencilTransform::default());
        assert!(!s.tiling);
        assert!(!s.follow_camera);
    }

    #[test]
    fn adopt_keeps_follow_camera_and_arm_disarm_tiling_never_touch_it() {
        let mut s = ProjectionState {
            follow_camera: true,
            ..ProjectionState::default()
        };
        let cam = head_on(glam::Vec3::ZERO, 5.0);
        s.arm(cam, view_rect());
        assert!(s.follow_camera && s.armed && s.frozen.is_none());
        s.tiling = true;
        assert!(s.follow_camera);
        s.disarm();
        assert!(s.follow_camera && !s.armed);
        s.adopt(
            PathBuf::from("a.png"),
            Ok(image(4, 2, |_, _| RED)),
            [200.0, 100.0],
        );
        assert!(s.follow_camera && !s.armed && s.frozen.is_none());
        s.adopt(
            PathBuf::from("b.png"),
            Err(ProjectionLoadError::Malformed {
                width: 0,
                height: 0,
                bytes: 0,
            }),
            [200.0, 100.0],
        );
        assert!(s.follow_camera);
        // Off stays off across a load and an arm (which then freezes).
        s.follow_camera = false;
        s.adopt(
            PathBuf::from("c.png"),
            Ok(image(2, 2, |_, _| RED)),
            [200.0, 100.0],
        );
        s.arm(cam, view_rect());
        assert!(!s.follow_camera && s.frozen.is_some());
    }

    #[test]
    fn adopt_keeps_tiling_and_arm_disarm_never_touch_it() {
        let mut s = ProjectionState {
            tiling: true,
            ..ProjectionState::default()
        };
        let cam = head_on(glam::Vec3::ZERO, 5.0);
        s.arm(cam, view_rect());
        assert!(s.tiling);
        s.disarm();
        assert!(s.tiling);
        // A successful load refits and disarms, but tiling is a
        // placement mode, not image data: it stays.
        s.adopt(
            PathBuf::from("a.png"),
            Ok(image(4, 2, |_, _| RED)),
            [200.0, 100.0],
        );
        assert!(s.tiling && !s.armed && s.frozen.is_none());
        // A refusal keeps it too; and off stays off across a load.
        s.adopt(
            PathBuf::from("b.png"),
            Err(ProjectionLoadError::Malformed {
                width: 0,
                height: 0,
                bytes: 0,
            }),
            [200.0, 100.0],
        );
        assert!(s.tiling);
        s.tiling = false;
        s.adopt(
            PathBuf::from("c.png"),
            Ok(image(2, 2, |_, _| RED)),
            [200.0, 100.0],
        );
        assert!(!s.tiling);
        s.arm(cam, view_rect());
        assert!(!s.tiling);
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
