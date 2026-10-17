//! The retouch tools: smudge + clone (docs/specs/smudge-clone-design.md,
//! §2's P1 row).
//!
//! Both are pure CPU ops over an RGBA8 [`RgbaBuffer`] — the same
//! tightly-packed, row-major, premultiplied bytes
//! `umber_gpu::paint::PaintTarget::read_back_rgba8` returns. The app keeps
//! a per-stroke CPU mirror of each touched tile (one readback at the
//! stroke's first touch), runs every dab through these ops, and writes the
//! dirty region back to the GPU target. No GPU code lives here, so every
//! op is testable headless.
//!
//! The dab footprint mirrors `umber_gpu::shaders::PAINT_COMPUTE_SHADER`
//! exactly: the texel box `[floor(pos - r), ceil(pos + r)]` clamped to the
//! target, texel centers at `+0.5`, `dist <= r`, and the same smoothstep
//! [`radial_falloff`] — so a retouch dab covers exactly the texels a paint
//! dab with the same position/radius/hardness would.
//!
//! The quick mask ([`crate::quick_mask`]) gates both ops at the same
//! point the shader gates a paint dab: the per-texel coverage is
//! multiplied by the mask there ([`smudge_dab_masked`],
//! [`clone_dab_masked`]).

use serde::{Deserialize, Serialize};

use crate::quick_mask::MaskView;

/// Fraction of the blur radius the motion tilt shifts the smudge sample
/// window by (back along the incoming stroke direction).
pub const SMUDGE_TILT_FACTOR: f32 = 0.5;

/// Smudge parameters (the design's `SmudgeParams`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SmudgeParams {
    /// The sample/write mix, 0..=1: 0 = pure original (the identity
    /// no-op), 1 = fully replaced by the smeared sample (before the
    /// brush's own alpha/falloff coverage scales it).
    pub strength: f32,
    /// The blur kernel radius factor, 0..=1: the box-blur half-width is
    /// `smear * radius` texels. Also scales the motion tilt, so
    /// `smear == 0` blurs nothing and shifts nothing.
    pub smear: f32,
}

impl Default for SmudgeParams {
    fn default() -> Self {
        Self {
            strength: 0.5,
            smear: 0.25,
        }
    }
}

/// Clone parameters (the design's `CloneParams`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CloneParams {
    /// Source point minus stroke-start point, in UV units. Set by the
    /// alt-click affordance (resolved against the next stroke's start) or
    /// edited directly in the panel.
    pub source_offset: (f32, f32),
    /// `true`: the source tracks the cursor (`cursor + offset`, the classic
    /// follow-stroke clone). `false`: every dab samples the fixed anchor
    /// (`stroke start + offset`).
    pub follow: bool,
}

impl Default for CloneParams {
    fn default() -> Self {
        Self {
            source_offset: (0.0, 0.0),
            follow: true,
        }
    }
}

/// Eraser parameters (the design's `EraseParams`, §2's P0 row).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct EraseParams {
    /// Which channels the erase touches. Default is alpha-only: color
    /// data is left in place (unassociated), so the stroke is
    /// non-destructive and undo restores the exact before-state.
    pub channels: EraseChannels,
}

/// Per-channel enable for the eraser (§2's P0 row).
///
/// An enabled channel is scaled toward zero by the dab's coverage
/// (`out = round(orig * (1 - coverage))`); a disabled channel is left
/// byte-identical. `a` scales the alpha channel (RGB untouched);
/// `r`/`g`/`b` scale that color channel toward zero leaving alpha
/// intact.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EraseChannels {
    pub r: bool,
    pub g: bool,
    pub b: bool,
    pub a: bool,
}

impl Default for EraseChannels {
    /// Alpha-only: the classic eraser (transparency, not color removal).
    fn default() -> Self {
        Self {
            r: false,
            g: false,
            b: false,
            a: true,
        }
    }
}

/// The active brush tool: what a stroke's dabs do to the target.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum BrushTool {
    /// The stamp: premultiplied alpha-over of the brush color (the GPU
    /// splat pass).
    #[default]
    Paint,
    /// Sample → blur → write back (CPU).
    Smudge(SmudgeParams),
    /// Sample the source point → write with the brush's flow (CPU).
    Clone(CloneParams),
    /// Scale the enabled channels toward zero by the dab's coverage
    /// (CPU, on the same per-stroke mirror as smudge/clone).
    Erase(EraseParams),
}

impl BrushTool {
    /// Whether this tool's dabs run through the CPU retouch path (a
    /// readback mirror + region write-back) instead of the GPU splat.
    pub fn is_retouch(&self) -> bool {
        !matches!(self, Self::Paint)
    }
}

/// A tightly-packed RGBA8 image (`width * height * 4` bytes, row-major,
/// premultiplied) — the CPU mirror of one paint target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaBuffer {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl RgbaBuffer {
    /// Wraps `data`; `None` when its length is not `width * height * 4`.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        (data.len() == width as usize * height as usize * 4).then_some(Self {
            width,
            height,
            data,
        })
    }

    /// A fully transparent `width`x`height` image.
    pub fn transparent(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; width as usize * height as usize * 4],
        }
    }

    /// The texel at `(x, y)` (caller guarantees in-bounds).
    pub fn texel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = self.index(x, y);
        [
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]
    }

    /// Overwrites the texel at `(x, y)` (caller guarantees in-bounds).
    pub fn set_texel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        let i = self.index(x, y);
        self.data[i..i + 4].copy_from_slice(&rgba);
    }

    /// Copies `rect` out as tightly-packed RGBA8 rows.
    pub fn region(&self, rect: DirtyRect) -> Vec<u8> {
        let mut out = Vec::with_capacity(rect.byte_len());
        for y in rect.y0..rect.y1 {
            let start = self.index(rect.x0, y);
            let end = start + rect.width() as usize * 4;
            out.extend_from_slice(&self.data[start..end]);
        }
        out
    }

    /// Writes tightly-packed `rgba` rows into `rect` (the inverse of
    /// [`Self::region`]). A length mismatch writes nothing.
    pub fn write_region(&mut self, rect: DirtyRect, rgba: &[u8]) {
        if rgba.len() != rect.byte_len() {
            return;
        }
        let row = rect.width() as usize * 4;
        for (i, y) in (rect.y0..rect.y1).enumerate() {
            let start = self.index(rect.x0, y);
            self.data[start..start + row].copy_from_slice(&rgba[i * row..(i + 1) * row]);
        }
    }

    fn index(&self, x: u32, y: u32) -> usize {
        (y as usize * self.width as usize + x as usize) * 4
    }
}

/// A texel rectangle, half-open: `[x0, x1) x [y0, y1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl DirtyRect {
    pub fn width(&self) -> u32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> u32 {
        self.y1 - self.y0
    }

    /// RGBA8 bytes the rect spans.
    pub fn byte_len(&self) -> usize {
        self.width() as usize * self.height() as usize * 4
    }

    /// The bounding rect of both.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    /// `a ∪ b` where either may be absent.
    pub fn union_opt(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        match (a, b) {
            (Some(a), Some(b)) => Some(a.union(b)),
            (a, b) => a.or(b),
        }
    }
}

/// One retouch dab: the paint-space fields of `umber_gpu::paint::Dab` the
/// ops consume (the color is the tool's, not the brush's).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetouchDab {
    /// Center, in target texel space.
    pub pos: [f32; 2],
    /// Radius, in texels.
    pub radius: f32,
    /// Flow/opacity multiplier (pressure-shaped, as the adapter emits it).
    pub alpha: f32,
    /// Radial falloff: 1 hard edge, 0 softest.
    pub hardness: f32,
}

/// The paint shader's radial falloff, verbatim: 1 inside
/// `hardness * radius`, smoothstep down to 0 at `radius`.
pub fn radial_falloff(t: f32, hardness: f32) -> f32 {
    let denom = (1.0 - hardness).max(1e-4);
    let edge_t = ((t - hardness) / denom).clamp(0.0, 1.0);
    1.0 - edge_t * edge_t * (3.0 - 2.0 * edge_t)
}

/// The shader's per-dab texel box `[floor(pos - r), ceil(pos + r)]`
/// clamped to the image, as a half-open [`DirtyRect`]; `None` when the box
/// misses the image entirely.
///
/// Shared with the app-side paint-stroke undo capture
/// (`umber-app`'s `paint_state`): the union of one stroke's staged dab
/// footprints is that stroke's dirty rect, so paint strokes snapshot the
/// same box a retouch dab would report. Kept next to the ops (not
/// duplicated in the app) so the box cannot drift from the shader math.
pub fn footprint(pos: [f32; 2], radius: f32, width: u32, height: u32) -> Option<DirtyRect> {
    if width == 0 || height == 0 {
        return None;
    }
    let r = radius.max(1e-5);
    let min_x = ((pos[0] - r).floor() as i64).max(0);
    let min_y = ((pos[1] - r).floor() as i64).max(0);
    let max_x = ((pos[0] + r).ceil() as i64).min(i64::from(width) - 1);
    let max_y = ((pos[1] + r).ceil() as i64).min(i64::from(height) - 1);
    if max_x < min_x || max_y < min_y {
        return None;
    }
    Some(DirtyRect {
        x0: min_x as u32,
        y0: min_y as u32,
        x1: max_x as u32 + 1,
        y1: max_y as u32 + 1,
    })
}

/// The per-texel coverage the paint shader would give `(x, y)` under
/// `dab` (`alpha * falloff`), or `None` outside the disc.
pub fn coverage(dab: &RetouchDab, x: u32, y: u32) -> Option<f32> {
    let r = dab.radius.max(1e-5);
    let cx = x as f32 + 0.5 - dab.pos[0];
    let cy = y as f32 + 0.5 - dab.pos[1];
    let dist = (cx * cx + cy * cy).sqrt();
    (dist <= r).then(|| dab.alpha * radial_falloff(dist / r, dab.hardness))
}

/// [`coverage`] gated by the quick mask: multiplied by `mask` at `(x, y)`
/// (the shader's `mask_gate` multiply, on the CPU). `None` mask = ungated.
fn gated_coverage(dab: &RetouchDab, x: u32, y: u32, mask: Option<MaskView<'_>>) -> Option<f32> {
    coverage(dab, x, y).map(|cov| mask.map_or(cov, |m| cov * m.at(x, y)))
}

/// The smudge sample-window shift for a dab moving along `motion` (texels
/// since the previous dab): back along the incoming direction by
/// `SMUDGE_TILT_FACTOR * smear * radius`, so each dab drags the color it
/// came from forward. Zero motion (a stroke's first dab) or zero smear
/// shifts nothing.
pub fn smudge_tilt(motion: [f32; 2], radius: f32, smear: f32) -> [i32; 2] {
    let len = (motion[0] * motion[0] + motion[1] * motion[1]).sqrt();
    if len <= f32::EPSILON || smear <= 0.0 {
        return [0, 0];
    }
    let reach = SMUDGE_TILT_FACTOR * smear * radius;
    [
        (-motion[0] / len * reach).round() as i32,
        (-motion[1] / len * reach).round() as i32,
    ]
}

/// The smudge op for one dab: sample the footprint, box-blur it by
/// `smear * radius` around a window tilted back along `motion`, and write
/// it back mixed by `strength * coverage` (coverage = the brush's
/// alpha × falloff). Blurs from a snapshot of the pre-dab texels (never
/// in place), clamping sample windows to the image (an edge texel averages
/// only the texels that exist). Premultiplied values blur as-is.
///
/// Returns the texels written, or `None` for the identity cases
/// (`strength <= 0`, `alpha <= 0`, or a zero-width blur with no tilt) —
/// which return before touching a byte, so the identity is exact.
pub fn smudge_dab(
    buf: &mut RgbaBuffer,
    dab: &RetouchDab,
    params: SmudgeParams,
    motion: [f32; 2],
) -> Option<DirtyRect> {
    smudge_dab_masked(buf, dab, params, motion, None)
}

/// [`smudge_dab`] gated by the quick mask: each texel's mix is scaled by
/// `mask` there (`None` = ungated, byte-identical to [`smudge_dab`]). The
/// mask gates the WRITE only — the blur still samples blocked texels, the
/// way a masked smudge drags color in from under the mask.
pub fn smudge_dab_masked(
    buf: &mut RgbaBuffer,
    dab: &RetouchDab,
    params: SmudgeParams,
    motion: [f32; 2],
    mask: Option<MaskView<'_>>,
) -> Option<DirtyRect> {
    let (rect, blur, tilt) = smudge_windows(dab, params, motion, buf.width, buf.height)?;
    // The table borrows `buf` only while building (its sums are owned),
    // so the write loop below holds it alone (E0502).
    let sat = smudge_table(buf, &rect, blur, tilt);
    smudge_loop(buf, &sat, dab, params, mask, rect, blur, tilt)
}

/// [`smudge_dab_masked`] sampling a separate `sample` image instead of the
/// destination buffer: the blur window reads `sample`, the mix writes
/// `buf` (per-layer-pool design §3 [M3] — a smudge into a layer plane
/// smears the visible composite, not the plane's own bytes, so
/// clone-to-new-layer's sibling workflow works for smudge too).
///
/// `sample` must match `buf`'s dimensions (the plane and its composite
/// cache are the same tile size by construction); a mismatch falls back
/// to the self-sampled [`smudge_dab_masked`] rather than sampling
/// misaligned rows.
pub fn smudge_dab_masked_sampled(
    buf: &mut RgbaBuffer,
    sample: &RgbaBuffer,
    dab: &RetouchDab,
    params: SmudgeParams,
    motion: [f32; 2],
    mask: Option<MaskView<'_>>,
) -> Option<DirtyRect> {
    if sample.width != buf.width || sample.height != buf.height {
        return smudge_dab_masked(buf, dab, params, motion, mask);
    }
    let (rect, blur, tilt) = smudge_windows(dab, params, motion, buf.width, buf.height)?;
    // The table borrows `sample` only while building (its sums are
    // owned), so the write loop below holds `buf` alone (E0502).
    let sat = smudge_table(sample, &rect, blur, tilt);
    smudge_loop(buf, &sat, dab, params, mask, rect, blur, tilt)
}

/// The smudge identity checks plus the dab footprint, blur half-width and
/// motion tilt. `None` when the dab writes nothing (strength 0,
/// alpha 0, zero-width blur with no tilt, or a footprint missing the
/// image) — the exact-identity early-outs.
fn smudge_windows(
    dab: &RetouchDab,
    params: SmudgeParams,
    motion: [f32; 2],
    width: u32,
    height: u32,
) -> Option<(DirtyRect, i64, [i32; 2])> {
    let strength = params.strength.clamp(0.0, 1.0);
    let smear = params.smear.clamp(0.0, 1.0);
    if strength <= 0.0 || dab.alpha <= 0.0 {
        return None;
    }
    let blur = (smear * dab.radius).round().max(0.0) as i64;
    let tilt = smudge_tilt(motion, dab.radius, smear);
    if blur == 0 && tilt == [0, 0] {
        return None;
    }
    let rect = footprint(dab.pos, dab.radius, width, height)?;
    Some((rect, blur, tilt))
}

/// The summed-area table over the sample region: the footprint grown by
/// the blur half-width and the tilt, clamped to the image. A table over
/// it gives every window's average in O(1).
fn smudge_table(sample: &RgbaBuffer, rect: &DirtyRect, blur: i64, tilt: [i32; 2]) -> SummedArea {
    let (w, h) = (i64::from(sample.width), i64::from(sample.height));
    let reach_x = blur + i64::from(tilt[0].unsigned_abs());
    let reach_y = blur + i64::from(tilt[1].unsigned_abs());
    let sx0 = (i64::from(rect.x0) - reach_x).max(0);
    let sy0 = (i64::from(rect.y0) - reach_y).max(0);
    let sx1 = (i64::from(rect.x1) + reach_x).min(w);
    let sy1 = (i64::from(rect.y1) + reach_y).min(h);
    SummedArea::build(sample, sx0, sy0, sx1, sy1)
}

/// Runs one dab's mix from the prebuilt `sat` into `buf` over `rect`
/// (symmetry siblings are the caller's loop — see the app-side stroke).
/// The argument count is the algorithm's shape (buffer + table + dab +
/// params + mask + the smudge window's three members); bundling the
/// window trio saves nothing at one call site.
#[allow(clippy::too_many_arguments)]
fn smudge_loop(
    buf: &mut RgbaBuffer,
    sat: &SummedArea,
    dab: &RetouchDab,
    params: SmudgeParams,
    mask: Option<MaskView<'_>>,
    rect: DirtyRect,
    blur: i64,
    tilt: [i32; 2],
) -> Option<DirtyRect> {
    let strength = params.strength.clamp(0.0, 1.0);
    let mut wrote = false;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let Some(cov) = gated_coverage(dab, x, y, mask) else {
                continue;
            };
            let mix = strength * cov;
            if mix <= 0.0 {
                continue;
            }
            // The window centered on the tilted texel, half-open.
            let cx = i64::from(x) + i64::from(tilt[0]);
            let cy = i64::from(y) + i64::from(tilt[1]);
            let (ax, ay) = (cx - blur, cy - blur);
            let (bx, by) = (cx + blur + 1, cy + blur + 1);
            let Some(avg) = sat.average(ax, ay, bx, by) else {
                continue;
            };
            let orig = buf.texel(x, y);
            let out: [u8; 4] = std::array::from_fn(|c| {
                let o = f32::from(orig[c]);
                (o + (avg[c] - o) * mix).round().clamp(0.0, 255.0) as u8
            });
            buf.set_texel(x, y, out);
            wrote = true;
        }
    }
    wrote.then_some(rect)
}

/// The clone op for one dab: every covered texel at `t` samples
/// `source` at `t + round(src_pos - dab.pos)` and composites it with the
/// brush's normal flow — premultiplied alpha-over at `coverage`, the paint
/// shader's exact blend (`src * cov + dst * (1 - src.a * cov)`). Source
/// texels off the image are skipped (v1: no wrap, no cross-tile reach).
///
/// `source` is the stroke-start snapshot, never the live buffer: a source
/// window overlapping the destination would otherwise re-clone its own
/// output (the feedback smear).
pub fn clone_dab(
    dst: &mut RgbaBuffer,
    source: &RgbaBuffer,
    dab: &RetouchDab,
    src_pos: [f32; 2],
) -> Option<DirtyRect> {
    clone_dab_masked(dst, source, dab, src_pos, None)
}

/// [`clone_dab`] gated by the quick mask: each texel's coverage is scaled
/// by `mask` at the DESTINATION texel (`None` = ungated, byte-identical
/// to [`clone_dab`]).
pub fn clone_dab_masked(
    dst: &mut RgbaBuffer,
    source: &RgbaBuffer,
    dab: &RetouchDab,
    src_pos: [f32; 2],
    mask: Option<MaskView<'_>>,
) -> Option<DirtyRect> {
    if dab.alpha <= 0.0 {
        return None;
    }
    let rect = footprint(dab.pos, dab.radius, dst.width, dst.height)?;
    let dx = (src_pos[0] - dab.pos[0]).round() as i64;
    let dy = (src_pos[1] - dab.pos[1]).round() as i64;
    let mut wrote = false;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let Some(cov) = gated_coverage(dab, x, y, mask) else {
                continue;
            };
            let sx = i64::from(x) + dx;
            let sy = i64::from(y) + dy;
            let in_source = (0..i64::from(source.width)).contains(&sx)
                && (0..i64::from(source.height)).contains(&sy);
            if !in_source {
                continue;
            }
            let s = source.texel(sx as u32, sy as u32);
            let d = dst.texel(x, y);
            let src_a = f32::from(s[3]) / 255.0 * cov;
            let out: [u8; 4] = std::array::from_fn(|c| {
                let sc = f32::from(s[c]) * cov;
                let dc = f32::from(d[c]) * (1.0 - src_a);
                (sc + dc).round().clamp(0.0, 255.0) as u8
            });
            dst.set_texel(x, y, out);
            wrote = true;
        }
    }
    wrote.then_some(rect)
}

/// Inpainting parameters (Wave 9's `wave-texture-inpainting`; the
/// healing-brush class). `search_radius` bounds the patch-match spiral
/// (how far from the naive source the fill may migrate to find a
/// better boundary match); `candidates` caps the search (the coarse
/// v1 budget — a full multiscale search is the follow-up when the
/// core earns it).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InpaintParams {
    /// Half-extent of the candidate spiral, in texels.
    pub search_radius: i32,
    /// Candidate offsets examined per dab.
    pub candidates: u32,
}

impl Default for InpaintParams {
    fn default() -> Self {
        // The coarse v1: a 12-texel spiral over 96 candidates. Small
        // enough for a per-dab budget on the CPU mirror; large enough
        // to jump a boundary misalignment at common radii.
        Self {
            search_radius: 12,
            candidates: 96,
        }
    }
}

/// Boundary-ring sample count for the patch score (4 pairs × 2 sides).
const INPAINT_SCORE_PAIRS: usize = 4;

/// One dab of the inpaint brush: fills the dab's disc from the
/// best-matching patch near the sampled region, blended by the same
/// gated coverage as [`clone_dab_masked`] — so the footprint, the
/// falloff and the quick mask behave identically to every other
/// retouch tool.
///
/// The patch match (v1, deterministic): candidate offsets walk a
/// fixed spiral around the source center's delta; each candidate is
/// scored by SSD over [`INPAINT_SCORE_PAIRS`] texel pairs straddling
/// the disc boundary — the ring where continuity is visible — plus the
/// disc's own texels (a candidate that matches the disc's PARTIAL
/// state, e.g. after earlier dabs of the same stroke, keeps the
/// stroke's own fills). The lowest-scoring offset wins; ties resolve
/// to the FIRST candidate (spiral order = distance order, so the
/// naive delta wins ties = smoothest fill).
///
/// Returns the dirty rect, or `None` when the dab covers nothing.
pub fn inpaint_dab(
    dst: &mut RgbaBuffer,
    source: &RgbaBuffer,
    dab: &RetouchDab,
    sample_center: [f32; 2],
    params: &InpaintParams,
    mask: Option<MaskView<'_>>,
) -> Option<DirtyRect> {
    if dab.alpha <= 0.0 {
        return None;
    }
    let rect = footprint(dab.pos, dab.radius, dst.width, dst.height)?;
    // The base delta: the naive "copy from the sampled region".
    let base_dx = (sample_center[0] - dab.pos[0]).round() as i64;
    let base_dy = (sample_center[1] - dab.pos[1]).round() as i64;
    // The boundary ring: texels just OUTSIDE the disc (where the
    // original content lives) paired with their inside-counterparts
    // (disc edge). Sampled sparsely for a bounded score.
    let score_pairs = boundary_pairs(dab, rect, dst.width, dst.height);
    let mut best = (u64::MAX, base_dx, base_dy);
    'search: for (examined, (ox, oy)) in spiral(params.search_radius).into_iter().enumerate() {
        if examined as u32 >= params.candidates {
            break 'search;
        }
        let dx = base_dx + i64::from(ox);
        let dy = base_dy + i64::from(oy);
        if let Some(s) = patch_score(dst, source, &score_pairs, dx, dy) {
            if s < best.0 {
                best = (s, dx, dy);
            }
        }
    }
    let (_, dx, dy) = best;
    // Fill: the clone_dab blend math with the matched delta.
    let mut wrote = false;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let Some(cov) = gated_coverage(dab, x, y, mask) else {
                continue;
            };
            let sx = i64::from(x) + dx;
            let sy = i64::from(y) + dy;
            let in_source = (0..i64::from(source.width)).contains(&sx)
                && (0..i64::from(source.height)).contains(&sy);
            if !in_source {
                continue;
            }
            let s = source.texel(sx as u32, sy as u32);
            let d = dst.texel(x, y);
            let src_a = f32::from(s[3]) / 255.0 * cov;
            let out: [u8; 4] = std::array::from_fn(|c| {
                let sc = f32::from(s[c]) * cov;
                let dc = f32::from(d[c]) * (1.0 - src_a);
                (sc + dc).round().clamp(0.0, 255.0) as u8
            });
            dst.set_texel(x, y, out);
            wrote = true;
        }
    }
    wrote.then_some(rect)
}

/// The boundary pairs for the score: `INPAINT_SCORE_PAIRS` samples on
/// the disc's edge ring, each paired with the texel just OUTSIDE the
/// footprint along the same ray (the continuity evidence).
fn boundary_pairs(
    dab: &RetouchDab,
    rect: DirtyRect,
    width: u32,
    height: u32,
) -> Vec<([i64; 2], [i64; 2])> {
    let cx = dab.pos[0];
    let cy = dab.pos[1];
    let r = dab.radius.max(0.5);
    let mut pairs = Vec::with_capacity(INPAINT_SCORE_PAIRS);
    for k in 0..INPAINT_SCORE_PAIRS {
        let theta = k as f32 * std::f32::consts::TAU / INPAINT_SCORE_PAIRS as f32;
        let (ux, uy) = (theta.cos(), theta.sin());
        let inside_x = cx + ux * (r - 1.0);
        let inside_y = cy + uy * (r - 1.0);
        let outside_x = cx + ux * (r + 1.0);
        let outside_y = cy + uy * (r + 1.0);
        let inside = [inside_x.floor() as i64, inside_y.floor() as i64];
        let outside = [outside_x.floor() as i64, outside_y.floor() as i64];
        // Both must land in the target AND outside the rect excluded
        // bounds (the pair must straddle: inside within the rect's
        // covered area is fine; outside must be a real texel).
        let ok = |p: [i64; 2]| {
            p[0] >= i64::from(rect.x0) && p[0] < i64::from(rect.x1)
                || (p[0] >= 0 && p[0] < i64::from(width))
        };
        let _ = ok;
        if inside[0] >= 0
            && inside[0] < i64::from(width)
            && inside[1] >= 0
            && inside[1] < i64::from(height)
            && outside[0] >= 0
            && outside[0] < i64::from(width)
            && outside[1] >= 0
            && outside[1] < i64::from(height)
        {
            pairs.push((inside, outside));
        }
    }
    pairs
}

/// SSD of one candidate: the four channel sums over each pair's
/// (source-under-inside vs dst-under-inside) plus the boundary
/// continuity (source-under-outside vs dst-under-outside). `None`
/// when a candidate reads out of the source (skip it).
fn patch_score(
    dst: &RgbaBuffer,
    source: &RgbaBuffer,
    pairs: &[([i64; 2], [i64; 2])],
    dx: i64,
    dy: i64,
) -> Option<u64> {
    let mut score = 0u64;
    for (inside, outside) in pairs {
        let si = (inside[0] + dx, inside[1] + dy);
        let so = (outside[0] + dx, outside[1] + dy);
        if si.0 < 0
            || si.1 < 0
            || si.0 >= i64::from(source.width)
            || si.1 >= i64::from(source.height)
            || so.0 < 0
            || so.1 < 0
            || so.0 >= i64::from(source.width)
            || so.1 >= i64::from(source.height)
        {
            return None;
        }
        let di = dst.texel(inside[0] as u32, inside[1] as u32);
        let do_ = dst.texel(outside[0] as u32, outside[1] as u32);
        let ssi = source.texel(si.0 as u32, si.1 as u32);
        let sso = source.texel(so.0 as u32, so.1 as u32);
        for c in 0..4 {
            let a = i64::from(ssi[c]) - i64::from(di[c]);
            score += (a * a) as u64;
            let b = i64::from(sso[c]) - i64::from(do_[c]);
            score += (b * b) as u64;
        }
    }
    Some(score)
}

/// The candidate spiral: every offset within the Chebyshev `radius`,
/// ordered by distance then by a fixed angle (atan2 buckets), so the
/// search is deterministic and distance-biased — ties resolve to the
/// nearest candidate, and equal distances resolve in a fixed order.
fn spiral(radius: i32) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            out.push((dx, dy));
        }
    }
    out.sort_by(|a, b| {
        let da = i64::from(a.0) * a.0 as i64 + i64::from(a.1) * a.1 as i64;
        let db = i64::from(b.0) * b.0 as i64 + i64::from(b.1) * b.1 as i64;
        da.cmp(&db).then_with(|| {
            let angle = |(x, y): (i32, i32)| {
                // A stable fixed angle: atan2 bucketed to a sortable
                // integer, with the 0-angle first (east, then
                // counterclockwise).
                let a = f32::atan2(y as f32, x as f32);
                let deg = a.to_degrees();
                let norm = if deg < 0.0 { deg + 360.0 } else { deg };
                (norm * 1000.0) as i32
            };
            angle(*a).cmp(&angle(*b))
        })
    });
    out
}

/// The eraser op for one dab: every covered texel's ENABLED channels are
/// scaled toward zero by the dab's coverage (`alpha * falloff`, the paint
/// shader's exact footprint math) — `out = round(orig * (1 - coverage))`.
/// Disabled channels are left byte-identical. The scale is multiplicative,
/// so a soft eraser at 50% flow halves the alpha (repeated dabs converge
/// to zero, like a real eraser) and a full-opacity hard dab zeroes its
/// disc exactly.
///
/// Only the layer's own channels are touched (alpha-only by default) and
/// the caller records the stroke-start snapshot for undo — that is the
/// row's "non-destructive" contract: no clear, no layer delete, and the
/// before-state is restorable.
///
/// On a premultiplied layer-owned plane use
/// [`erase_dab_masked_premultiplied`] instead (per-layer-pool design §3
/// [M1]): this op scales alpha alone and would leave `p > a`.
///
/// Returns the texels written, or `None` for the identity cases
/// (`alpha <= 0`, no channel enabled, or zero coverage everywhere) —
/// which return before touching a byte, so the identity is exact.
pub fn erase_dab(buf: &mut RgbaBuffer, dab: &RetouchDab, params: EraseParams) -> Option<DirtyRect> {
    erase_dab_masked(buf, dab, params, None)
}

/// [`erase_dab`] gated by the quick mask: each texel's coverage is scaled
/// by `mask` at that texel (`None` = ungated, byte-identical to
/// [`erase_dab`]) — the same gate point the paint shader uses.
pub fn erase_dab_masked(
    buf: &mut RgbaBuffer,
    dab: &RetouchDab,
    params: EraseParams,
    mask: Option<MaskView<'_>>,
) -> Option<DirtyRect> {
    if dab.alpha <= 0.0 {
        return None;
    }
    let keep = [
        !params.channels.r,
        !params.channels.g,
        !params.channels.b,
        !params.channels.a,
    ];
    if keep == [true; 4] {
        return None;
    }
    let rect = footprint(dab.pos, dab.radius, buf.width, buf.height)?;
    let mut wrote = false;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let Some(cov) = gated_coverage(dab, x, y, mask) else {
                continue;
            };
            let k = (1.0 - cov.clamp(0.0, 1.0)).clamp(0.0, 1.0);
            if k >= 1.0 {
                continue;
            }
            let orig = buf.texel(x, y);
            let out: [u8; 4] = std::array::from_fn(|c| {
                if keep[c] {
                    orig[c]
                } else {
                    (f32::from(orig[c]) * k).round().clamp(0.0, 255.0) as u8
                }
            });
            if out != orig {
                buf.set_texel(x, y, out);
                wrote = true;
            }
        }
    }
    wrote.then_some(rect)
}

/// The eraser op for one dab on a PREMULTIPLIED layer-owned plane
/// (per-layer-pool design §3 [M1]): every write preserves `p ≤ a` per
/// channel (valid premultiplied data), where [`erase_dab_masked`] would
/// scale alpha alone and leave `p > a`.
///
/// * `EraseChannels.a` scales ALL FOUR channels by `k = 1 − coverage`:
///   the premultiplied erase (alpha goes to zero and so does the
///   premultiplied color, so requirement row 19 holds literally).
/// * `EraseChannels.r/g/b` mean straight-space color removal: the texel
///   is unpremultiplied (`s = clamp(p / a)`, `a == 0` leaves the texel
///   untouched — there is no color to remove), the enabled color
///   channels are scaled by `k`, and the result is re-premultiplied
///   under the (possibly `a`-scaled) alpha. An `a`-enabled erase scales
///   the color channels directly, so the `r/g/b` flags are redundant
///   beside it, never contradictory.
/// * The result clamps `p` to `a` per channel either way.
///
/// On Base ([`erase_dab_masked`]'s straight semantics) this op is never
/// used: Base keeps today's bytes exactly (the §1.4 exemption).
///
/// Returns the texels written, or `None` for the identity cases
/// (`alpha <= 0`, no channel enabled, or zero coverage everywhere).
pub fn erase_dab_premultiplied(
    buf: &mut RgbaBuffer,
    dab: &RetouchDab,
    params: EraseParams,
) -> Option<DirtyRect> {
    erase_dab_masked_premultiplied(buf, dab, params, None)
}

/// [`erase_dab_premultiplied`] gated by the quick mask: each texel's
/// coverage is scaled by `mask` at that texel (`None` = ungated,
/// byte-identical to [`erase_dab_premultiplied`]) — the same gate point
/// the paint shader uses.
pub fn erase_dab_masked_premultiplied(
    buf: &mut RgbaBuffer,
    dab: &RetouchDab,
    params: EraseParams,
    mask: Option<MaskView<'_>>,
) -> Option<DirtyRect> {
    if dab.alpha <= 0.0 {
        return None;
    }
    if !(params.channels.r || params.channels.g || params.channels.b || params.channels.a) {
        return None;
    }
    let rect = footprint(dab.pos, dab.radius, buf.width, buf.height)?;
    let mut wrote = false;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let Some(cov) = gated_coverage(dab, x, y, mask) else {
                continue;
            };
            let k = (1.0 - cov.clamp(0.0, 1.0)).clamp(0.0, 1.0);
            if k >= 1.0 {
                continue;
            }
            let orig = buf.texel(x, y);
            let mut out = orig;
            if params.channels.a {
                // The premultiplied erase: alpha scales all four channels.
                // `k <= 1` preserves `p ≤ a` (`round` is monotonic), and
                // the clamp below keeps it exact in f32.
                for c in 0..4 {
                    out[c] = (f32::from(orig[c]) * k).round().clamp(0.0, 255.0) as u8;
                }
            } else {
                let a = f32::from(orig[3]);
                if a > 0.0 {
                    let enabled = [params.channels.r, params.channels.g, params.channels.b];
                    for c in 0..3 {
                        if !enabled[c] {
                            continue;
                        }
                        let straight = (f32::from(orig[c]) / a).clamp(0.0, 1.0);
                        out[c] = (straight * k * a).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
            for c in 0..3 {
                out[c] = out[c].min(out[3]);
            }
            if out != orig {
                buf.set_texel(x, y, out);
                wrote = true;
            }
        }
    }
    wrote.then_some(rect)
}

/// The per-stroke clone source tracker, in tile-local texel space (the
/// app converts the UV offset with the target's texel scale and V flip
/// before building it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloneStroke {
    /// `stroke start + offset`: what every fixed-mode dab samples.
    anchor: [f32; 2],
    /// Source minus cursor, texels.
    offset: [f32; 2],
    follow: bool,
}

impl CloneStroke {
    /// Starts tracking at `start` (the stroke's first cursor position)
    /// with `offset` texels to the source.
    pub fn begin(start: [f32; 2], offset: [f32; 2], follow: bool) -> Self {
        Self {
            anchor: [start[0] + offset[0], start[1] + offset[1]],
            offset,
            follow,
        }
    }

    /// The source point the dab at `cursor` samples: `cursor + offset`
    /// when following, else the fixed anchor.
    pub fn source_for(&self, cursor: [f32; 2]) -> [f32; 2] {
        if self.follow {
            [cursor[0] + self.offset[0], cursor[1] + self.offset[1]]
        } else {
            self.anchor
        }
    }

    /// The fixed anchor (`stroke start + offset`).
    pub fn anchor(&self) -> [f32; 2] {
        self.anchor
    }
}

/// A summed-area table over one RGBA8 region, for O(1) box averages.
struct SummedArea {
    /// Region origin in image texels.
    x0: i64,
    y0: i64,
    /// Region size.
    w: i64,
    h: i64,
    /// `(w + 1) * (h + 1)` per-channel prefix sums (row 0 / col 0 zero).
    sums: Vec<[u64; 4]>,
}

impl SummedArea {
    fn build(buf: &RgbaBuffer, x0: i64, y0: i64, x1: i64, y1: i64) -> Self {
        let w = (x1 - x0).max(0);
        let h = (y1 - y0).max(0);
        let stride = (w + 1) as usize;
        let mut sums = vec![[0u64; 4]; stride * (h + 1) as usize];
        for j in 0..h {
            let mut row = [0u64; 4];
            for i in 0..w {
                let t = buf.texel((x0 + i) as u32, (y0 + j) as u32);
                for (acc, v) in row.iter_mut().zip(t) {
                    *acc += u64::from(v);
                }
                let above = sums[j as usize * stride + (i + 1) as usize];
                sums[(j + 1) as usize * stride + (i + 1) as usize] =
                    std::array::from_fn(|c| above[c] + row[c]);
            }
        }
        Self { x0, y0, w, h, sums }
    }

    /// Mean of the half-open image-space box `[ax, bx) x [ay, by)`
    /// intersected with the region; `None` when the intersection is empty.
    fn average(&self, ax: i64, ay: i64, bx: i64, by: i64) -> Option<[f32; 4]> {
        let ax = (ax - self.x0).clamp(0, self.w);
        let ay = (ay - self.y0).clamp(0, self.h);
        let bx = (bx - self.x0).clamp(0, self.w);
        let by = (by - self.y0).clamp(0, self.h);
        if bx <= ax || by <= ay {
            return None;
        }
        let stride = (self.w + 1) as usize;
        let at = |x: i64, y: i64| self.sums[y as usize * stride + x as usize];
        let (s11, s01, s10, s00) = (at(bx, by), at(ax, by), at(bx, ay), at(ax, ay));
        let n = ((bx - ax) * (by - ay)) as f32;
        // `s11 + s00 >= s01 + s10` for prefix sums, so add before
        // subtracting (u64 never underflows).
        Some(std::array::from_fn(|c| {
            (s11[c] + s00[c] - s01[c] - s10[c]) as f32 / n
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: u32 = 64;
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    /// Opaque black left half (x < 32), opaque white right half: a
    /// vertical hard edge between columns 31 and 32.
    fn hard_edge() -> RgbaBuffer {
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                buf.set_texel(x, y, if x < SIZE / 2 { BLACK } else { WHITE });
            }
        }
        buf
    }

    fn hard_dab(pos: [f32; 2], radius: f32) -> RetouchDab {
        RetouchDab {
            pos,
            radius,
            alpha: 1.0,
            hardness: 1.0,
        }
    }

    /// Texels whose bytes differ between `a` and `b`.
    fn changed(a: &RgbaBuffer, b: &RgbaBuffer) -> usize {
        a.data
            .chunks_exact(4)
            .zip(b.data.chunks_exact(4))
            .filter(|(x, y)| x != y)
            .count()
    }

    /// Design test 1: a hard edge stroked across at strength 1 blurs the
    /// boundary columns (before/after delta asserted at the edge), and
    /// strength 0 is the exact identity.
    #[test]
    fn smudge_blurs_a_hard_edge_and_strength_zero_is_identity() {
        let before = hard_edge();
        let params = SmudgeParams {
            strength: 1.0,
            smear: 0.25,
        };
        // A stroke across the edge (dabs every 4 texels along row 32,
        // moving +x from the black side into the white).
        let mut after = before.clone();
        let mut prev: Option<[f32; 2]> = None;
        for i in 0..9 {
            let pos = [16.0 + 4.0 * i as f32, 32.0];
            let motion = prev.map_or([0.0, 0.0], |p| [pos[0] - p[0], pos[1] - p[1]]);
            smudge_dab(&mut after, &hard_dab(pos, 8.0), params, motion);
            prev = Some(pos);
        }
        // The edge's two columns at the stroke's middle row: black lifted,
        // white dropped — the boundary is no longer hard.
        let row = 32;
        let left = after.texel(31, row);
        let right = after.texel(32, row);
        assert_eq!(before.texel(31, row), BLACK);
        assert_eq!(before.texel(32, row), WHITE);
        assert!(
            left[0] > 20,
            "black side of the edge must lighten: {left:?}"
        );
        assert!(
            right[0] < 235,
            "white side of the edge must darken: {right:?}"
        );
        // Alpha stays opaque (both sides were opaque).
        assert_eq!(left[3], 255);
        assert_eq!(right[3], 255);
        // Far from the edge the blur window sees one color only: untouched.
        assert_eq!(after.texel(4, row), BLACK);
        assert_eq!(after.texel(60, row), WHITE);

        // strength = 0: the identity, byte for byte, and no dirty rect.
        let mut identity = before.clone();
        let zero = SmudgeParams {
            strength: 0.0,
            smear: 1.0,
        };
        assert_eq!(
            smudge_dab(
                &mut identity,
                &hard_dab([32.0, 32.0], 8.0),
                zero,
                [0.0, 4.0]
            ),
            None
        );
        assert_eq!(identity, before);
    }

    /// Design test 2: the affected-texel count is monotonic in the smear
    /// radius. Smear values keep the blur half-width (smear * r = 2, 4, 6,
    /// 8 texels) under the footprint so the count grows strictly.
    #[test]
    fn smudge_wider_smear_spreads_further() {
        let before = hard_edge();
        let dab = hard_dab([32.0, 32.0], 16.0);
        let mut counts = Vec::new();
        for smear in [0.125, 0.25, 0.375, 0.5] {
            let mut after = before.clone();
            smudge_dab(
                &mut after,
                &dab,
                SmudgeParams {
                    strength: 1.0,
                    smear,
                },
                [0.0, 0.0],
            );
            counts.push(changed(&before, &after));
        }
        assert!(
            counts[0] > 0,
            "the narrowest smear must still blur: {counts:?}"
        );
        assert!(
            counts.windows(2).all(|w| w[0] < w[1]),
            "affected texels must grow strictly with the smear radius: {counts:?}"
        );
        // smear = 0 with no motion: nothing to blur, nothing written.
        let mut none = before.clone();
        let zero = SmudgeParams {
            strength: 1.0,
            smear: 0.0,
        };
        assert_eq!(smudge_dab(&mut none, &dab, zero, [0.0, 0.0]), None);
        assert_eq!(none, before);
    }

    #[test]
    fn smudge_tilt_points_back_along_the_motion() {
        // Moving +x: the window shifts -x (toward where the stroke came
        // from) by 0.5 * smear * radius.
        assert_eq!(smudge_tilt([3.0, 0.0], 16.0, 0.5), [-4, 0]);
        assert_eq!(smudge_tilt([0.0, -2.0], 16.0, 0.5), [0, 4]);
        // First dab (no motion) and zero smear: no tilt.
        assert_eq!(smudge_tilt([0.0, 0.0], 16.0, 0.5), [0, 0]);
        assert_eq!(smudge_tilt([3.0, 0.0], 16.0, 0.0), [0, 0]);
    }

    /// A source image where every texel is unique: r = x * 4, g = y * 4.
    fn coordinate_image() -> RgbaBuffer {
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                buf.set_texel(x, y, [(x * 4) as u8, (y * 4) as u8, 0, 255]);
            }
        }
        buf
    }

    /// Asserts the hard dab at `dst_center` reproduced the source patch
    /// at `src_center` texel for texel.
    fn assert_patch_equals(
        out: &RgbaBuffer,
        src: &RgbaBuffer,
        dst_center: [u32; 2],
        src_center: [u32; 2],
        half: u32,
    ) {
        for j in 0..=2 * half {
            for i in 0..=2 * half {
                let d = out.texel(dst_center[0] - half + i, dst_center[1] - half + j);
                let s = src.texel(src_center[0] - half + i, src_center[1] - half + j);
                assert_eq!(d, s, "patch texel ({i},{j}) must match the source");
            }
        }
    }

    /// Design test 3: the dab at the cursor reproduces the patch at
    /// cursor + offset (two identical dabs), and with follow = false a
    /// moving cursor keeps sampling the SAME source.
    #[test]
    fn clone_fixed_samples_the_same_source() {
        let source = coordinate_image();
        let mut out = source.clone();
        let offset = [20.0, 8.0];
        let start = [16.0, 16.0];
        let stroke = CloneStroke::begin(start, offset, false);

        // First dab: the patch at the cursor equals the patch at
        // cursor + offset (the source).
        let src0 = stroke.source_for(start);
        assert_eq!(src0, [36.0, 24.0]);
        clone_dab(&mut out, &source, &hard_dab(start, 4.0), src0);
        assert_patch_equals(&out, &source, [16, 16], [36, 24], 2);

        // The cursor moves; the fixed source does not.
        let moved = [16.0, 40.0];
        let src1 = stroke.source_for(moved);
        assert_eq!(src1, src0, "follow = false must keep the same source");
        assert_eq!(src1, stroke.anchor());
        clone_dab(&mut out, &source, &hard_dab(moved, 4.0), src1);
        // The second dab stamped the SAME source patch again.
        assert_patch_equals(&out, &source, [16, 40], [36, 24], 2);
    }

    /// Design test 4: with follow = true the sampled origin tracks the
    /// stroke — its trajectory is the cursor's, shifted by the offset.
    #[test]
    fn clone_follow_source_tracks_the_stroke() {
        let source = coordinate_image();
        let mut out = source.clone();
        let offset = [20.0, -6.0];
        let cursors = [[10.0, 30.0], [14.0, 31.0], [18.0, 33.0], [22.0, 36.0]];
        let stroke = CloneStroke::begin(cursors[0], offset, true);
        let trajectory: Vec<[f32; 2]> = cursors.iter().map(|&c| stroke.source_for(c)).collect();
        for (c, s) in cursors.iter().zip(&trajectory) {
            assert_eq!(*s, [c[0] + offset[0], c[1] + offset[1]]);
        }
        // The source moved exactly as far as the cursor did, step by step.
        for (cw, sw) in cursors.windows(2).zip(trajectory.windows(2)) {
            assert_eq!(
                [sw[1][0] - sw[0][0], sw[1][1] - sw[0][1]],
                [cw[1][0] - cw[0][0], cw[1][1] - cw[0][1]]
            );
        }
        // And each dab reproduced its own (moving) source patch.
        for (c, s) in cursors.iter().zip(&trajectory) {
            clone_dab(&mut out, &source, &hard_dab(*c, 1.5), *s);
            let center = [c[0] as u32, c[1] as u32];
            let src = [s[0] as u32, s[1] as u32];
            assert_eq!(
                out.texel(center[0], center[1]),
                source.texel(src[0], src[1])
            );
        }
    }

    #[test]
    fn clone_skips_off_image_source_and_blends_with_flow() {
        let source = coordinate_image();
        // Source entirely off the image: nothing written.
        let mut out = RgbaBuffer::transparent(SIZE, SIZE);
        assert_eq!(
            clone_dab(&mut out, &source, &hard_dab([8.0, 8.0], 4.0), [-100.0, 8.0]),
            None
        );
        assert!(out.data.iter().all(|&b| b == 0));
        // Half flow over transparent: premultiplied halves.
        let dab = RetouchDab {
            alpha: 0.5,
            ..hard_dab([8.0, 8.0], 4.0)
        };
        clone_dab(&mut out, &source, &dab, [8.0, 8.0]);
        let s = source.texel(8, 8);
        let o = out.texel(8, 8);
        assert_eq!(o[3], 128, "half flow of an opaque source: {o:?}");
        assert_eq!(o[0], (f32::from(s[0]) * 0.5).round() as u8);
    }

    #[test]
    fn region_round_trips_and_union_bounds() {
        let mut buf = coordinate_image();
        let rect = DirtyRect {
            x0: 3,
            y0: 5,
            x1: 9,
            y1: 7,
        };
        let bytes = buf.region(rect);
        assert_eq!(bytes.len(), rect.byte_len());
        let original = buf.clone();
        buf.write_region(rect, &vec![0; rect.byte_len()]);
        assert_ne!(buf, original);
        buf.write_region(rect, &bytes);
        assert_eq!(buf, original);
        let other = DirtyRect {
            x0: 1,
            y0: 6,
            x1: 4,
            y1: 20,
        };
        assert_eq!(
            DirtyRect::union_opt(Some(rect), Some(other)),
            Some(DirtyRect {
                x0: 1,
                y0: 5,
                x1: 9,
                y1: 20
            })
        );
        assert_eq!(DirtyRect::union_opt(None, Some(rect)), Some(rect));
    }

    /// Opaque mid-gray image: every channel is erasable and readable.
    fn gray_opaque() -> RgbaBuffer {
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                buf.set_texel(x, y, [200, 120, 60, 255]);
            }
        }
        buf
    }

    fn erase_all() -> EraseParams {
        EraseParams {
            channels: EraseChannels {
                r: true,
                g: true,
                b: true,
                a: true,
            },
        }
    }

    /// Test (a): a full-opacity hard erase zeroes the enabled channels
    /// under the footprint and leaves everything else byte-identical.
    #[test]
    fn erase_full_opacity_zeroes_alpha_under_the_footprint() {
        let before = gray_opaque();
        let mut after = before.clone();
        let rect = erase_dab(
            &mut after,
            &hard_dab([32.0, 32.0], 8.0),
            EraseParams::default(),
        )
        .expect("erase must write");
        // The disc center is fully covered: alpha exactly zero.
        assert_eq!(after.texel(32, 32)[3], 0, "center alpha must be zero");
        // Default is alpha-only: color channels untouched.
        assert_eq!(&after.texel(32, 32)[..3], &[200, 120, 60]);
        // Far outside the radius: byte-identical.
        assert_eq!(after.texel(4, 4), before.texel(4, 4));
        assert_eq!(after.texel(60, 60), before.texel(60, 60));
        // The dirty rect is the dab's shader-identical footprint box.
        assert_eq!(rect, footprint([32.0, 32.0], 8.0, SIZE, SIZE).unwrap());
    }

    /// Test (b): a 50%-flow erase halves the alpha (coverage-scaled, the
    /// same falloff math as paint) — the multiplicative converge-to-zero
    /// semantics, not a binary clear.
    #[test]
    fn erase_half_flow_halves_alpha() {
        let before = gray_opaque();
        let mut after = before.clone();
        let dab = RetouchDab {
            alpha: 0.5,
            ..hard_dab([32.0, 32.0], 8.0)
        };
        erase_dab(&mut after, &dab, EraseParams::default()).expect("erase must write");
        let a = after.texel(32, 32)[3];
        assert!(
            (126..=129).contains(&a),
            "half flow of 255 must land near 128, got {a}"
        );
        // A second identical dab converges further (multiplicative).
        erase_dab(&mut after, &dab, EraseParams::default()).expect("erase must write");
        let a2 = after.texel(32, 32)[3];
        assert!(
            a2 < a,
            "repeated dabs must converge toward zero: {a} -> {a2}"
        );
        assert!(
            (62..=66).contains(&a2),
            "two half-flow dabs land near 64, got {a2}"
        );
    }

    /// Test (c): the per-channel flags gate which channels the erase
    /// touches. Alpha-only erases alpha leaving color; RGB erases color
    /// leaving alpha.
    #[test]
    fn erase_channel_flags_gate_which_channels_change() {
        // A-only (the default): alpha goes, color stays.
        let mut a_only = gray_opaque();
        erase_dab(
            &mut a_only,
            &hard_dab([32.0, 32.0], 8.0),
            EraseParams::default(),
        );
        assert_eq!(a_only.texel(32, 32), [200, 120, 60, 0]);

        // RGB-only: color goes, alpha stays.
        let mut rgb_only = gray_opaque();
        let rgb = EraseParams {
            channels: EraseChannels {
                r: true,
                g: true,
                b: true,
                a: false,
            },
        };
        erase_dab(&mut rgb_only, &hard_dab([32.0, 32.0], 8.0), rgb);
        assert_eq!(rgb_only.texel(32, 32), [0, 0, 0, 255]);

        // Single channel: only R moves.
        let mut r_only = gray_opaque();
        let r = EraseParams {
            channels: EraseChannels {
                r: true,
                g: false,
                b: false,
                a: false,
            },
        };
        erase_dab(&mut r_only, &hard_dab([32.0, 32.0], 8.0), r);
        assert_eq!(r_only.texel(32, 32), [0, 120, 60, 255]);

        // All channels: everything zeroes.
        let mut all = gray_opaque();
        erase_dab(&mut all, &hard_dab([32.0, 32.0], 8.0), erase_all());
        assert_eq!(all.texel(32, 32), [0, 0, 0, 0]);

        // No channel enabled, or zero flow: the exact identity.
        let mut none = gray_opaque();
        let no_channels = EraseParams {
            channels: EraseChannels {
                r: false,
                g: false,
                b: false,
                a: false,
            },
        };
        assert_eq!(
            erase_dab(&mut none, &hard_dab([32.0, 32.0], 8.0), no_channels),
            None
        );
        assert_eq!(
            erase_dab(
                &mut none,
                &RetouchDab {
                    alpha: 0.0,
                    ..hard_dab([32.0, 32.0], 8.0)
                },
                erase_all()
            ),
            None
        );
        assert_eq!(none, gray_opaque());
    }

    /// Per-layer-pool §8 T22: the premultiplied erase golden. Layer-plane
    /// texel `(200,0,0,255)`, alpha-only erase at exactly 0.5 coverage
    /// (`k = 0.5`, exact in f32) → `(100,0,0,128)`: `200·0.5 = 100.0`
    /// exactly, and `255·0.5 = 127.5` is a TIE, which `f32::round`
    /// rounds half away from zero (a defined rule, not
    /// implementation-defined) → 128.
    #[test]
    fn premultiplied_erase_alpha_scales_all_four_channels() {
        // A hard dab at texel-center with alpha 0.5: `falloff(0, 1) = 1`
        // exactly, so the center texel's coverage is exactly 0.5.
        let dab = RetouchDab {
            alpha: 0.5,
            ..hard_dab([32.0, 32.0], 8.0)
        };
        assert_eq!(
            coverage(&dab, 32, 32),
            Some(0.5),
            "the golden's coverage must be exactly 0.5 first"
        );
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        buf.set_texel(32, 32, [200, 0, 0, 255]);
        erase_dab_premultiplied(&mut buf, &dab, EraseParams::default()).expect("erase must write");
        assert_eq!(buf.texel(32, 32), [100, 0, 0, 128]);
        // Companion: the same erase on straight (Base) semantics gives
        // today's `(200,0,0,128)` byte-for-byte (§1.4's exemption).
        let mut base = RgbaBuffer::transparent(SIZE, SIZE);
        base.set_texel(32, 32, [200, 0, 0, 255]);
        erase_dab(&mut base, &dab, EraseParams::default()).expect("erase must write");
        assert_eq!(base.texel(32, 32), [200, 0, 0, 128]);
    }

    /// The `r/g/b` flags on a premultiplied plane mean straight-space
    /// color removal: unpremultiply, scale the enabled channels by `k`,
    /// re-premultiply. `(100,0,0,128)` at `k = 0.5`, red-only:
    /// `s = 100/128 = 0.78125`, `0.78125·0.5·128 = 50.0` exactly.
    #[test]
    fn premultiplied_erase_rgb_unpremultiplies_scales_and_repremultiplies() {
        let dab = RetouchDab {
            alpha: 0.5,
            ..hard_dab([32.0, 32.0], 8.0)
        };
        let red_only = EraseParams {
            channels: EraseChannels {
                r: true,
                g: false,
                b: false,
                a: false,
            },
        };
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        buf.set_texel(32, 32, [100, 0, 0, 128]);
        erase_dab_masked_premultiplied(&mut buf, &dab, red_only, None).expect("erase must write");
        assert_eq!(buf.texel(32, 32), [50, 0, 0, 128]);
        // Alpha-zero texels carry no color to remove: untouched.
        let mut zero = RgbaBuffer::transparent(SIZE, SIZE);
        zero.set_texel(32, 32, [0, 0, 0, 0]);
        assert_eq!(
            erase_dab_masked_premultiplied(&mut zero, &dab, red_only, None),
            None,
            "an all-zero texel erases to itself: no write"
        );
        assert_eq!(zero.texel(32, 32), [0, 0, 0, 0]);
    }

    /// Invalid premultiplied input (`p > a`) composites as "clamp `p` to
    /// `a`": the rgb-flag path clamps the un-premultiply to `0..=1`, and
    /// every path clamps the result. `(200,0,0,128)` red-only at 0.5:
    /// `s = min(200/128, 1) = 1.0`, `1.0·0.5·128 = 64`.
    #[test]
    fn premultiplied_erase_clamps_p_to_a() {
        let dab = RetouchDab {
            alpha: 0.5,
            ..hard_dab([32.0, 32.0], 8.0)
        };
        let red_only = EraseParams {
            channels: EraseChannels {
                r: true,
                g: false,
                b: false,
                a: false,
            },
        };
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        buf.set_texel(32, 32, [200, 0, 0, 128]);
        erase_dab_masked_premultiplied(&mut buf, &dab, red_only, None).expect("erase must write");
        assert_eq!(buf.texel(32, 32), [64, 0, 0, 128]);
    }

    /// The sampled smudge (§3 [M3]): the blur reads the sample image and
    /// writes the destination, so a smudge into an empty plane drags the
    /// visible composite's color in.
    #[test]
    fn sampled_smudge_blurs_the_sample_into_the_buffer() {
        let sample = hard_edge();
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        let params = SmudgeParams {
            strength: 1.0,
            smear: 0.25,
        };
        let dab = hard_dab([32.0, 32.0], 8.0);
        smudge_dab_masked_sampled(&mut buf, &sample, &dab, params, [0.0, 0.0], None)
            .expect("sampled smudge must write");
        assert_ne!(buf, RgbaBuffer::transparent(SIZE, SIZE));
        // The sample is never written.
        assert_eq!(sample, hard_edge());
        // The destination started transparent: every written texel's color
        // came from the sample's blur, mixed over transparent black.
        let t = buf.texel(32, 32);
        assert!(
            t[0] > 0 && t[0] < 255,
            "the edge blur lands mid-gray: {t:?}"
        );
        assert_eq!(t[3], 255, "the opaque sample blurs to opaque");
        // A size-mismatched sample falls back to the self-sampled path
        // (byte-identical to `smudge_dab_masked`).
        let small = RgbaBuffer::transparent(8, 8);
        let mut fallback = hard_edge();
        let mut classic = hard_edge();
        let r1 = smudge_dab_masked_sampled(&mut fallback, &small, &dab, params, [0.0, 0.0], None);
        let r2 = smudge_dab_masked(&mut classic, &dab, params, [0.0, 0.0], None);
        assert_eq!(r1, r2);
        assert_eq!(fallback, classic);
    }

    #[test]
    fn footprint_matches_the_shader_box() {
        // pos 2,2 r 6 → floor(-4) clamps to 0, ceil(8) → 8 inclusive.
        // (Stage-4 tests above: T22's premultiplied-erase golden, the
        // rgb-flag unpremultiply path, the p≤a clamp, and the sampled
        // smudge live just above this anchor.)
        assert_eq!(
            footprint([2.0, 2.0], 6.0, 64, 64),
            Some(DirtyRect {
                x0: 0,
                y0: 0,
                x1: 9,
                y1: 9
            })
        );
        assert_eq!(footprint([-50.0, 2.0], 6.0, 64, 64), None);
    }

    // ---- wave9-inpaint-core -------------------------------------------------

    fn solid_buffer(w: u32, h: u32, rgba: [u8; 4]) -> RgbaBuffer {
        let data: Vec<u8> = rgba.repeat((w * h) as usize);
        RgbaBuffer::new(w, h, data).expect("solid dims")
    }

    #[test]
    fn inpaint_fills_a_hole_from_the_sampled_region() {
        // The classic healing case: a solid region with one hole,
        // sampled from clean solid. The fill must reproduce the solid.
        let mut dst = solid_buffer(32, 32, [90, 90, 90, 255]);
        for y in 12..20 {
            for x in 12..20 {
                dst.set_texel(x, y, [0, 0, 0, 255]);
            }
        }
        let source = solid_buffer(32, 32, [90, 90, 90, 255]);
        let dab = RetouchDab {
            pos: [15.5, 15.5],
            radius: 5.0,
            alpha: 1.0,
            hardness: 1.0,
        };
        let params = InpaintParams::default();
        let dirty = inpaint_dab(&mut dst, &source, &dab, [24.5, 24.5], &params, None);
        assert!(dirty.is_some(), "the dab wrote");
        // The hole's core is refilled to the solid.
        assert_eq!(dst.texel(15, 15), [90, 90, 90, 255]);
        assert_eq!(dst.texel(16, 16), [90, 90, 90, 255]);
    }

    #[test]
    fn inpaint_prefers_the_boundary_continuity_match() {
        // Two candidate regions: a WRONG-color solid and the RIGHT
        // one. The sampled center points at the wrong one, but the
        // boundary ring (which the score reads) matches the right
        // one — the search must migrate to continuity.
        let w = 48u32;
        // dst: gray left half, a hole at (15,24), distinct tan right.
        let mut dst = solid_buffer(w, 32, [100, 100, 100, 255]);
        for y in 0..32 {
            for x in 24..w {
                dst.set_texel(x, y, [180, 160, 120, 255]);
            }
        }
        for y in 12..20 {
            for x in 12..20 {
                dst.set_texel(x, y, [0, 0, 0, 255]);
            }
        }
        // source: the same two-tone layout WITHOUT the hole, shifted
        // so the naive delta points at the tan side; the boundary
        // evidence (gray surrounds the hole) must pull the match back
        // to the gray side.
        let mut source = solid_buffer(w, 32, [100, 100, 100, 255]);
        for y in 0..32 {
            for x in 24..w {
                source.set_texel(x, y, [180, 160, 120, 255]);
            }
        }
        let dab = RetouchDab {
            pos: [15.5, 15.5],
            radius: 4.0,
            alpha: 1.0,
            hardness: 1.0,
        };
        let params = InpaintParams {
            search_radius: 10,
            candidates: 400,
        };
        let _ = inpaint_dab(&mut dst, &source, &dab, [30.5, 15.5], &params, None);
        // The hole's core: filled from the gray (100) region, not the
        // naive tan (180) sample.
        let filled = dst.texel(15, 15);
        assert_eq!(
            filled,
            [100, 100, 100, 255],
            "boundary continuity wins over the naive sample: got {filled:?}"
        );
    }

    #[test]
    fn inpaint_respects_the_quick_mask() {
        let mut dst = solid_buffer(32, 32, [10, 20, 30, 255]);
        for y in 12..20 {
            for x in 12..20 {
                dst.set_texel(x, y, [0, 0, 0, 255]);
            }
        }
        let source = solid_buffer(32, 32, [10, 20, 30, 255]);
        let dab = RetouchDab {
            pos: [15.5, 15.5],
            radius: 5.0,
            alpha: 1.0,
            hardness: 1.0,
        };
        // A mask that blocks the texel (15,15) entirely (1.0 = allowed).
        let coverage: Vec<f32> = (0..32)
            .flat_map(|y| (0..32).map(move |x| if x == 15 && y == 15 { 0.0 } else { 1.0 }))
            .collect();
        let mask = MaskView::new(32, 32, &coverage).expect("mask dims");
        let dirty = inpaint_dab(
            &mut dst,
            &source,
            &dab,
            [24.5, 24.5],
            &InpaintParams::default(),
            Some(mask),
        );
        assert!(dirty.is_some());
        assert_eq!(
            dst.texel(15, 15),
            [0, 0, 0, 255],
            "the masked texel is untouched"
        );
        assert_eq!(
            dst.texel(16, 16),
            [10, 20, 30, 255],
            "the unmasked ones fill"
        );
    }

    #[test]
    fn inpaint_zero_alpha_dab_is_a_noop() {
        let mut dst = solid_buffer(16, 16, [1, 2, 3, 255]);
        let source = solid_buffer(16, 16, [9, 9, 9, 255]);
        let dab = RetouchDab {
            pos: [8.0, 8.0],
            radius: 4.0,
            alpha: 0.0,
            hardness: 1.0,
        };
        assert!(inpaint_dab(
            &mut dst,
            &source,
            &dab,
            [2.0, 2.0],
            &InpaintParams::default(),
            None
        )
        .is_none());
        assert_eq!(dst.texel(8, 8), [1, 2, 3, 255]);
    }

    #[test]
    fn the_spiral_is_deterministic_distance_ordered_and_complete() {
        let s = spiral(3);
        assert_eq!(s[0], (0, 0), "the naive delta is the first candidate");
        assert_eq!(s.len(), 49, "Chebyshev radius 3 covers 7x7");
        // Distance-ordered: every prefix is nondecreasing in squared
        // distance.
        let d = |(x, y): (i32, i32)| x * x + y * y;
        for k in 1..s.len() {
            assert!(d(s[k - 1]) <= d(s[k]), "spiral order broken at {k}");
        }
        // Deterministic: the same call gives the same order.
        assert_eq!(s, spiral(3));
    }
}
