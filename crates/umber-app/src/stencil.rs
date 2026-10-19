//! The stencil engine's session half (`docs/specs/stencil-engine-design.md`
//! "The design", sequencing step 2): the [`StencilState`] the shell owns,
//! the PNG load behind a memory cap, the gate bytes, and the composition
//! of the stencil gate with the quick-mask gate. The transform and the
//! rasterize math are `umber_core::stencil`'s; this module never
//! re-derives them.
//!
//! Judgment calls (the next kill extends from these):
//!
//! - **The decoder is `umber_graph::decode_png_rgba8`**, NOT
//!   `umber_export::png::read_png_rgba8` (the path the shell's other PNG
//!   loads use, e.g. main.rs's import publish and brush_panel.rs's stamp
//!   chooser). `read_png_rgba8` flips rows into GPU bottom-up order
//!   (umber-export png.rs, its "Row order" contract); the stencil lives in
//!   y-down screen space, so that loader would mirror it vertically.
//!   `decode_png_rgba8` returns the file's top-down rows. Both accept
//!   8-bit RGB/RGBA only; RGB loads as fully opaque (a solid stencil).
//! - **The memory cap** ([`DEFAULT_MAX_STENCIL_PIXELS`], 4096²) follows
//!   psd_import.rs's `MAX_PSD_SIDE` / `MAX_PSD_BYTES` precedent (an
//!   importer refuses an allocation-sized input with an honest error).
//!   The cap runs TWICE: on the IHDR's declared size before any decode
//!   allocation ([`png_dimensions`]), and on the decoded size (the
//!   decoder's dims are authoritative).
//! - **Composition is `min` per byte**, not bitwise `&`. For the binary
//!   v1 gate (0/255) both agree; `min` stays correct once the design's
//!   soft edges (Open question 3) put partial values in the stencil.
//! - **The composition order** (stencil first, quick mask folded in
//!   second) is a statement about who writes LAST, since `min` itself
//!   commutes: the quick mask's in-place edits republish rects of the
//!   gate texture (paint_state.rs `stamp_mask` -> `publish_mask`), so the
//!   quick mask must be applied on top of, never under, the stencil.
//! - **The gate target is the active tile's quick-mask plane size**
//!   (`PaintState::quick_mask_for_tile(active_tile())`): the quick mask
//!   is target-sized per set, and it is the plane the stencil composes
//!   with. Reset ([`StencilState::reset`]) fits the image to that same
//!   size, consistent with umber-core's v1 boundary "target texel space
//!   IS the transform's screen space". Reset keeps `fit_to_viewport`'s
//!   largest-fit scale (it may scale above 1); the design's chord
//!   section words Reset as "scale 1" — the core kill named the conflict,
//!   and this kill keeps the built behavior until the viewport chords
//!   (step 3) give Reset a real viewport rect.
//! - **A successful load activates the stencil and Resets it**: loading
//!   an image is the request to paint through it.
//! - **v1 boundary — the gate does NOT reach the GPU yet.** umber-paint
//!   has no public lane that queues caller bytes as a `WriteMask`
//!   (`publish_mask` / `sync_mask_tile` / `gate_bytes` are private,
//!   paint_state.rs:5184-5226); dabs only read the gate while
//!   `stroke_mask_gate` is armed, which only the quick mask or the cavity
//!   arms (paint_state.rs:1714); and every quick-mask resync re-uploads
//!   `gate_plane` (paint_state.rs:5158), which would overwrite shell-
//!   written bytes. The real hook is an umber-paint change — a stencil
//!   plane multiplied into `gate_plane` the way the cavity plane is
//!   (paint_state.rs:5171-5179), plus `|| stencil_gates()` at :1714. So
//!   [`StencilState::stage_gate`] builds the composed bytes the upload
//!   will carry and parks them in [`StencilState::staged`] (the seam);
//!   the panel says plainly that painting through it is pending.
//! - **The screen <-> UV gap** (stencil-core's judgment call 4) is carried
//!   forward unchanged: the gate is rasterized in target texel space,
//!   and projecting the screen-space quad onto UV texels is the hook
//!   kill's (or step 3's) job.

use fluent::FluentValue;
use std::path::{Path, PathBuf};
use thiserror::Error;
use umber_brush::QuickMask;
use umber_core::stencil::{
    fit_to_viewport, rasterize_gate, StencilError, StencilImage, StencilTransform,
};
use umber_paint::paint_state::PaintState;

/// The load-time cap on a stencil image's pixel count (4096², 64 MiB of
/// decoded RGBA8): larger images are refused, never decoded.
pub const DEFAULT_MAX_STENCIL_PIXELS: usize = 4096 * 4096;

/// The overlay's drawn opacity on a fresh session (the overlay, not the
/// mask: the gate stays binary whatever this is). One value with the
/// panel field's reset.
pub const DEFAULT_OVERLAY_OPACITY: f32 = crate::display_panel::DEFAULT_STENCIL_OPACITY;

/// Everything that can go wrong loading a stencil PNG.
#[derive(Debug, Error)]
pub enum StencilLoadError {
    /// The file could not be read.
    #[error("cannot read the stencil file: {0}")]
    Io(#[from] std::io::Error),
    /// Over the pixel cap (declared by the header or decoded).
    #[error("stencil image {width}x{height} exceeds the {max}-pixel cap")]
    TooLarge {
        /// The image width.
        width: u32,
        /// The image height.
        height: u32,
        /// The cap it exceeded.
        max: usize,
    },
    /// Not a PNG the decoder accepts (8-bit RGB/RGBA, non-interlaced).
    #[error("stencil decode failed: {0}")]
    Decode(#[from] umber_graph::ImageError),
    /// The decoded bytes did not build a coverage image.
    #[error("stencil image rejected: {0}")]
    Image(#[from] StencilError),
}

impl StencilLoadError {
    /// The catalog key the panel shows for this refusal (one key per
    /// reason — the `import.reason-*` precedent). Decode and image
    /// rejections share "unsupported": both mean "not an image we read".
    pub fn reason_key(&self) -> &'static str {
        match self {
            Self::Io(_) => "overlays.stencil-unreadable",
            Self::TooLarge { .. } => "overlays.stencil-too-large",
            Self::Decode(_) | Self::Image(_) => "overlays.stencil-unsupported",
        }
    }

    /// The refusal rendered through the catalogs (the English detail
    /// stays in the log line).
    pub fn localized(&self) -> String {
        match self {
            Self::TooLarge {
                width: w,
                height: h,
                max: cap,
            } => crate::i18n::tr_args(
                self.reason_key(),
                [
                    ("width", FluentValue::from(i64::from(*w))),
                    ("height", FluentValue::from(i64::from(*h))),
                    ("max", FluentValue::from(*cap as i64)),
                ],
            ),
            _ => crate::i18n::tr(self.reason_key()),
        }
    }
}

/// The composed gate the next upload will carry (the seam's output; see
/// the module docs' v1 boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedGate {
    /// Gate width, texels (the target's).
    pub width: u32,
    /// Gate height, texels.
    pub height: u32,
    /// `width * height` bytes, row-major, `255` = paint allowed.
    pub values: Vec<u8>,
}

impl StagedGate {
    /// The share of texels the gate opens (any nonzero byte), whole
    /// percent rounded down; `0` for an empty gate.
    pub fn open_percent(&self) -> u32 {
        if self.values.is_empty() {
            return 0;
        }
        let open = self.values.iter().filter(|&&v| v != 0).count();
        (open * 100 / self.values.len()) as u32
    }
}

/// The session's stencil: the loaded image, where it sits, whether it
/// gates, and how strongly the overlay draws. Session state, never
/// serialized (v1: one global stencil — the design's Open question 2).
#[derive(Debug, Clone, PartialEq)]
pub struct StencilState {
    /// The coverage image (`None` until a load succeeds).
    pub image: Option<StencilImage>,
    /// Where the image sits in target texel space.
    pub transform: StencilTransform,
    /// Whether the stencil gates paint (the panel's checkbox; the N key
    /// toggles it once the viewport chords land).
    pub active: bool,
    /// The overlay quad's drawn opacity, `0..=1`.
    pub overlay_opacity: f32,
    /// The file the image came from (the panel's read-only path label).
    pub path: Option<PathBuf>,
    /// The last load's refusal, localized (cleared by the next success).
    pub load_error: Option<String>,
    /// The composed gate the last change built (the upload seam).
    pub staged: Option<StagedGate>,
}

impl Default for StencilState {
    /// Inactive, no image, the identity-ish default transform, the
    /// overlay at [`DEFAULT_OVERLAY_OPACITY`].
    fn default() -> Self {
        Self {
            image: None,
            transform: StencilTransform::default(),
            active: false,
            overlay_opacity: DEFAULT_OVERLAY_OPACITY,
            path: None,
            load_error: None,
            staged: None,
        }
    }
}

impl StencilState {
    /// The stencil's gate at `target_w` x `target_h`: `None` while
    /// inactive or with no image, else the rasterized bytes.
    pub fn gate_bytes(&self, target_w: u32, target_h: u32) -> Option<Vec<u8>> {
        if !self.active {
            return None;
        }
        let image = self.image.as_ref()?;
        Some(rasterize_gate(image, &self.transform, target_w, target_h))
    }

    /// Re-fits the image to a `view_w` x `view_h` view (the Reset
    /// button). A no-op with no image.
    pub fn reset(&mut self, view_w: f32, view_h: f32) {
        if let Some(image) = &self.image {
            self.transform = fit_to_viewport(image.width(), image.height(), view_w, view_h);
        }
    }

    /// Takes a load's outcome: success adopts the image, remembers
    /// `path`, activates and Resets to the view; a refusal keeps the
    /// previous image and records the reason.
    pub fn adopt(
        &mut self,
        path: PathBuf,
        result: Result<StencilImage, StencilLoadError>,
        view: [f32; 2],
    ) {
        match result {
            Ok(image) => {
                self.image = Some(image);
                self.path = Some(path);
                self.load_error = None;
                self.active = true;
                self.reset(view[0], view[1]);
            }
            Err(err) => {
                log::warn!("stencil {} refused: {err}", path.display());
                self.load_error = Some(err.localized());
            }
        }
    }

    /// THE UPLOAD SEAM (TODO(stencil-paint-hook)): composes the gate for
    /// `target` with the quick-mask bytes `quick` and parks it in
    /// [`Self::staged`]. The hook kill hands `staged` to umber-paint's
    /// gate lane instead (see the module docs' v1 boundary). Nothing is
    /// staged without a target, or while the stencil contributes no gate.
    pub fn stage_gate(&mut self, target: Option<(u32, u32)>, quick: Option<&[u8]>) {
        self.staged = target.and_then(|(width, height)| {
            let stencil = self.gate_bytes(width, height)?;
            let values = compose_gate(Some(&stencil), quick)?;
            Some(StagedGate {
                width,
                height,
                values,
            })
        });
    }
}

/// The shell's composition-order contract: the byte the gate texture
/// holds for a stencil gate and a quick-mask gate of the same size.
/// `None` when neither gates; a single gate passes through; both compose
/// as per-byte `min` (AND for binary gates), stencil first and the quick
/// mask folded in second. Mismatched lengths fail CLOSED (every texel
/// blocked, at the stencil's length) — nothing ever paints ungated.
pub fn compose_gate(stencil: Option<&[u8]>, quick: Option<&[u8]>) -> Option<Vec<u8>> {
    match (stencil, quick) {
        (None, None) => None,
        (Some(only), None) | (None, Some(only)) => Some(only.to_vec()),
        (Some(stencil), Some(quick)) => {
            let (len, quick_len) = (stencil.len(), quick.len());
            if len != quick_len {
                log::warn!("stencil gate {len} B vs quick mask {quick_len} B: blocking");
                return Some(vec![0; len]);
            }
            let mut gate = stencil.to_vec();
            for (g, &q) in gate.iter_mut().zip(quick) {
                *g = (*g).min(q);
            }
            Some(gate)
        }
    }
}

/// `tile`'s quick-mask gate as bytes, or `None` while the mask doesn't
/// gate (`QuickMask::gates`: armed and not in mask mode). Mirrors the
/// quick-mask half of umber-paint's private `gate_plane` + `gate_bytes`
/// (paint_state.rs:5158-5199): an untouched tile reads the mask's empty
/// value; coverage clamps to `0..=1` and rounds to the byte. The cavity
/// factor is umber-paint's and is not mirrored here.
pub fn quick_mask_gate_bytes(mask: &QuickMask, tile: u16) -> Option<Vec<u8>> {
    if !mask.gates() {
        return None;
    }
    let (w, h) = mask.dimensions();
    let len = w as usize * h as usize;
    let to_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(match mask.coverage(tile) {
        Some(plane) if plane.len() == len => plane.iter().copied().map(to_byte).collect(),
        _ => vec![to_byte(mask.empty_value()); len],
    })
}

/// Refuses a `width` x `height` image over `max_pixels` (also when the
/// product overflows `usize`).
pub fn check_stencil_size(
    width: u32,
    height: u32,
    max_pixels: usize,
) -> Result<(), StencilLoadError> {
    let pixels = (width as usize).checked_mul(height as usize);
    match pixels {
        Some(n) if n <= max_pixels => Ok(()),
        _ => Err(StencilLoadError::TooLarge {
            width,
            height,
            max: max_pixels,
        }),
    }
}

/// The IHDR's declared `(width, height)`: the big-endian u32s at bytes
/// 16..24, valid when bytes 12..16 spell `IHDR` (IHDR is always the
/// first chunk after the 8-byte signature + 4-byte length). `None` for
/// anything shorter or different — the decoder reports those.
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

/// Builds the coverage image from PNG `bytes`, refusing anything over
/// `max_pixels` before decoding (the header's declared size) and after
/// (the decoded size).
pub fn stencil_from_png_bytes(
    bytes: &[u8],
    max_pixels: usize,
) -> Result<StencilImage, StencilLoadError> {
    if let Some((width, height)) = png_dimensions(bytes) {
        check_stencil_size(width, height, max_pixels)?;
    }
    let (width, height, rgba) = umber_graph::decode_png_rgba8(bytes)?;
    check_stencil_size(width, height, max_pixels)?;
    Ok(StencilImage::from_alpha(width, height, &rgba)?)
}

/// Loads a stencil PNG from `path` under [`DEFAULT_MAX_STENCIL_PIXELS`]
/// (alpha `>= 128` paints through; see `StencilImage::from_alpha`).
pub fn load_png_rgba(path: &Path) -> Result<StencilImage, StencilLoadError> {
    let bytes = std::fs::read(path)?;
    stencil_from_png_bytes(&bytes, DEFAULT_MAX_STENCIL_PIXELS)
}

/// The Display panel's stencil section (the Overlays popover hosts the
/// same rows once the viewport kill lands — step 3): draws
/// [`crate::display_panel::stencil_rows`] over `state`, loads a picked
/// file, Resets, and re-stages the gate after every gate-affecting
/// change (load, toggle, Reset — not opacity, which only the overlay
/// reads). The one egui-touching fn here: a thin adapter.
pub fn panel_section(ui: &mut egui::Ui, state: &mut StencilState, paint: Option<&PaintState>) {
    let tile = paint.map(|p| p.active_tile());
    let target = paint
        .zip(tile)
        .map(|(p, t)| p.quick_mask_for_tile(t).dimensions());
    // No paint session: fit to the image's own size (scale 1).
    let view = match (target, &state.image) {
        (Some((w, h)), _) => [w as f32, h as f32],
        (None, Some(image)) => [image.width() as f32, image.height() as f32],
        (None, None) => [0.0, 0.0],
    };
    let staged = state
        .staged
        .as_ref()
        .map(|g| (g.width, g.height, g.open_percent()));
    let response = crate::display_panel::stencil_rows(
        ui,
        crate::display_panel::StencilRow {
            active: &mut state.active,
            opacity: &mut state.overlay_opacity,
            path: state.path.as_deref(),
            has_image: state.image.is_some(),
            load_error: state.load_error.as_deref(),
            staged,
        },
    );
    let mut restage = response.toggled;
    if let Some(path) = response.picked {
        let result = load_png_rgba(&path);
        // The picked file's own dims are only known after the load.
        let view = match (target, &result) {
            (None, Ok(image)) => [image.width() as f32, image.height() as f32],
            _ => view,
        };
        state.adopt(path, result, view);
        restage = true;
    }
    if response.reset {
        state.reset(view[0], view[1]);
        restage = true;
    }
    if restage {
        let quick = paint
            .zip(tile)
            .and_then(|(p, t)| quick_mask_gate_bytes(p.quick_mask_for_tile(t), t));
        state.stage_gate(target, quick.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0/1 rows -> RGBA with alpha 0/255 (RGB at 200: a wrong-channel
    /// read would open every texel).
    fn rgba(rows: &[&[u8]]) -> Vec<u8> {
        rows.iter()
            .flat_map(|r| r.iter())
            .flat_map(|&on| [200, 200, 200, on * 255])
            .collect()
    }

    /// The core suite's asymmetric 4x4 pattern.
    const PATTERN: [[u8; 4]; 4] = [[1, 0, 0, 0], [1, 1, 0, 0], [0, 1, 1, 0], [0, 0, 0, 1]];

    fn pattern_image() -> StencilImage {
        let rows: Vec<&[u8]> = PATTERN.iter().map(|r| r.as_slice()).collect();
        StencilImage::from_alpha(4, 4, &rgba(&rows)).unwrap()
    }

    fn pattern_bytes() -> Vec<u8> {
        PATTERN.iter().flatten().map(|&on| on * 255).collect()
    }

    /// The image center on texel (2, 2), unit scale: target texels ==
    /// image texels.
    const IDENTITY_4X4: StencilTransform = StencilTransform {
        center: [2.0, 2.0],
        scale: 1.0,
        rotation: 0.0,
    };

    /// PNG bytes -> image under the default cap.
    fn load(bytes: &[u8]) -> Result<StencilImage, StencilLoadError> {
        stencil_from_png_bytes(bytes, DEFAULT_MAX_STENCIL_PIXELS)
    }

    fn active_pattern() -> StencilState {
        StencilState {
            image: Some(pattern_image()),
            transform: IDENTITY_4X4,
            active: true,
            ..StencilState::default()
        }
    }

    #[test]
    fn default_state_is_inactive_empty_at_half_opacity() {
        let s = StencilState::default();
        assert!(!s.active);
        assert_eq!(s.overlay_opacity, 0.5);
        assert_eq!(s.transform, StencilTransform::default());
        assert!(s.image.is_none() && s.path.is_none());
        assert!(s.load_error.is_none() && s.staged.is_none());
    }

    #[test]
    fn gate_bytes_need_both_an_image_and_activation() {
        let inactive = StencilState {
            active: false,
            ..active_pattern()
        };
        assert_eq!(inactive.gate_bytes(4, 4), None);
        let imageless = StencilState {
            active: true,
            ..StencilState::default()
        };
        assert_eq!(imageless.gate_bytes(4, 4), None);
        // Active + identity: the image's own bytes.
        assert_eq!(active_pattern().gate_bytes(4, 4), Some(pattern_bytes()));
    }

    #[test]
    fn compose_is_the_and_table_and_passes_single_gates_through() {
        assert_eq!(compose_gate(None, None), None);
        let s = [255, 255, 0, 0];
        let q = [255, 0, 255, 0];
        assert_eq!(compose_gate(Some(&s), None), Some(s.to_vec()));
        assert_eq!(compose_gate(None, Some(&q)), Some(q.to_vec()));
        // 255&255 = 255, 255&0 = 0, 0&255 = 0, 0&0 = 0.
        assert_eq!(compose_gate(Some(&s), Some(&q)), Some(vec![255, 0, 0, 0]));
        // Soft values compose by min (bitwise & would give 128&64 = 0).
        assert_eq!(compose_gate(Some(&[128]), Some(&[64])), Some(vec![64]));
        // Mismatched sizes fail closed at the stencil's length.
        assert_eq!(compose_gate(Some(&s), Some(&[255; 3])), Some(vec![0; 4]));
    }

    #[test]
    fn the_size_guard_refuses_past_the_cap() {
        let max = DEFAULT_MAX_STENCIL_PIXELS;
        assert!(check_stencil_size(4096, 4096, max).is_ok());
        assert!(matches!(
            check_stencil_size(4097, 4096, max),
            Err(StencilLoadError::TooLarge {
                width: 4097,
                height: 4096,
                max: DEFAULT_MAX_STENCIL_PIXELS,
            })
        ));
        // 6 px against caps of 6 and 5: the boundary is inclusive.
        assert!(check_stencil_size(3, 2, 6).is_ok());
        assert!(check_stencil_size(3, 2, 5).is_err());
        assert!(check_stencil_size(u32::MAX, u32::MAX, max).is_err());
    }

    #[test]
    fn an_oversized_header_is_refused_before_decoding() {
        // Signature + an IHDR declaring 5000x5000 with a junk CRC and no
        // data: the decoder would report a CRC error, so TooLarge proves
        // the header peek ran first.
        let mut bytes = vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13];
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&5000u32.to_be_bytes());
        bytes.extend_from_slice(&5000u32.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(png_dimensions(&bytes), Some((5000, 5000)));
        let err = load(&bytes).unwrap_err();
        assert!(
            matches!(
                err,
                StencilLoadError::TooLarge {
                    width: 5000,
                    height: 5000,
                    ..
                }
            ),
            "{err:?}"
        );
        // Under a generous cap the same bytes reach the decoder.
        let err = stencil_from_png_bytes(&bytes, usize::MAX).unwrap_err();
        assert!(matches!(err, StencilLoadError::Decode(_)), "{err:?}");
    }

    #[test]
    fn png_bytes_load_top_down_with_the_alpha_cut() {
        // 2x2, alphas 0/255 on top, 127/128 below: the cut is >= 128.
        let px = [[200, 200, 200, 0], [200, 200, 200, 255]];
        let low = [[200, 200, 200, 127], [200, 200, 200, 128]];
        let raw: Vec<u8> = px.iter().chain(low.iter()).flatten().copied().collect();
        let png = umber_graph::encode_png_rgba8(2, 2, &raw).unwrap();
        let image = load(&png).unwrap();
        assert_eq!((image.width(), image.height()), (2, 2));
        // Row 0 is the file's first row (a bottom-up loader gives
        // [0, 255, 0, 255] here).
        assert_eq!(image.coverage(), &[0, 255, 0, 255]);
        let tall_rows: [&[u8]; 2] = [&[1], &[0]];
        let tall = umber_graph::encode_png_rgba8(1, 2, &rgba(&tall_rows)).unwrap();
        let image = load(&tall).unwrap();
        assert_eq!(image.coverage(), &[255, 0]);
        // The decoded size is capped too (2 px against a cap of 1).
        assert!(matches!(
            stencil_from_png_bytes(&tall, 1),
            Err(StencilLoadError::TooLarge {
                width: 1,
                height: 2,
                max: 1,
            })
        ));
    }

    #[test]
    fn a_missing_file_is_an_io_refusal() {
        let missing = Path::new("no-such-dir/no-such-stencil.png");
        let err = load_png_rgba(missing).unwrap_err();
        assert!(matches!(err, StencilLoadError::Io(_)), "{err:?}");
    }

    #[test]
    fn adopt_activates_and_fits_or_keeps_the_old_image() {
        let mut s = StencilState::default();
        let opaque = vec![255; 400 * 100 * 4];
        let wide = StencilImage::from_alpha(400, 100, &opaque).unwrap();
        s.adopt(PathBuf::from("logo.png"), Ok(wide), [200.0, 300.0]);
        assert!(s.active);
        assert_eq!(s.path.as_deref(), Some(Path::new("logo.png")));
        // fit_to_viewport(400, 100, 200, 300): scale 0.5, centered.
        assert_eq!(
            s.transform,
            StencilTransform {
                center: [100.0, 150.0],
                scale: 0.5,
                rotation: 0.0,
            }
        );
        let before = s.clone();
        let refusal = Err(StencilLoadError::TooLarge {
            width: 9000,
            height: 9000,
            max: DEFAULT_MAX_STENCIL_PIXELS,
        });
        s.adopt(PathBuf::from("huge.png"), refusal, [200.0, 300.0]);
        assert_eq!(s.image, before.image);
        assert_eq!(s.path, before.path);
        assert_eq!(s.transform, before.transform);
        // The rendered text is the catalog's (Fluent may wrap the
        // placeables in isolation marks, so only presence is pinned
        // here; the key itself is pinned below).
        assert!(s.load_error.is_some());
    }

    #[test]
    fn each_refusal_maps_to_its_own_catalog_key() {
        let io = std::io::Error::from(std::io::ErrorKind::NotFound);
        let cases = [
            (StencilLoadError::Io(io), "overlays.stencil-unreadable"),
            (
                StencilLoadError::TooLarge {
                    width: 1,
                    height: 2,
                    max: 1,
                },
                "overlays.stencil-too-large",
            ),
            (
                StencilLoadError::Decode(umber_graph::ImageError::Png("x".into())),
                "overlays.stencil-unsupported",
            ),
            (
                StencilLoadError::Image(StencilError::EmptyImage {
                    width: 0,
                    height: 1,
                }),
                "overlays.stencil-unsupported",
            ),
        ];
        let i18n = crate::i18n::I18n::builtin();
        for (err, key) in cases {
            assert_eq!(err.reason_key(), key, "{err:?}");
            for locale in ["en-US", "pt-BR"] {
                assert!(i18n.has_key(locale, key), "{locale} missing {key}");
            }
        }
    }

    #[test]
    fn reset_refits_and_is_a_no_op_without_an_image() {
        let mut s = active_pattern();
        s.reset(8.0, 16.0);
        // 4x4 in 8x16: min(2, 4) = 2, center (4, 8).
        assert_eq!(
            s.transform,
            StencilTransform {
                center: [4.0, 8.0],
                scale: 2.0,
                rotation: 0.0,
            }
        );
        let mut empty = StencilState::default();
        empty.reset(8.0, 16.0);
        assert_eq!(empty.transform, StencilTransform::default());
    }

    #[test]
    fn quick_mask_bytes_follow_gating_and_the_empty_value() {
        let mut mask = QuickMask::new(2, 2);
        assert_eq!(quick_mask_gate_bytes(&mask, 1001), None, "not armed");
        mask.set_active(true);
        // Armed, untouched tile: the empty value (0 = blocked) everywhere.
        assert_eq!(quick_mask_gate_bytes(&mask, 1001), Some(vec![0; 4]));
        mask.set_editing(true);
        assert_eq!(quick_mask_gate_bytes(&mask, 1001), None, "mask mode");
    }

    #[test]
    fn stage_gate_composes_and_needs_a_target() {
        let mut s = active_pattern();
        s.stage_gate(Some((4, 4)), None);
        let staged = s.staged.clone().unwrap();
        assert_eq!((staged.width, staged.height), (4, 4));
        assert_eq!(staged.values, pattern_bytes());
        // PATTERN opens 6 of 16 texels: 37.5% -> 37.
        assert_eq!(staged.open_percent(), 37);
        // A fully blocking quick mask wins every texel.
        s.stage_gate(Some((4, 4)), Some(&[0; 16]));
        assert_eq!(s.staged.as_ref().unwrap().values, vec![0; 16]);
        assert_eq!(s.staged.as_ref().unwrap().open_percent(), 0);
        // No target, or an inactive stencil: nothing staged.
        s.stage_gate(None, None);
        assert_eq!(s.staged, None);
        s.active = false;
        s.stage_gate(Some((4, 4)), Some(&[255; 16]));
        assert_eq!(s.staged, None);
    }
}
