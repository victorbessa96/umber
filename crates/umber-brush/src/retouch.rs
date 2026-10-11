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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EraseParams {
    /// Which channels the erase touches. Default is alpha-only: color
    /// data is left in place (unassociated), so the stroke is
    /// non-destructive and undo restores the exact before-state.
    pub channels: EraseChannels,
}

impl Default for EraseParams {
    fn default() -> Self {
        Self {
            channels: EraseChannels::default(),
        }
    }
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
pub(crate) fn footprint(pos: [f32; 2], radius: f32, width: u32, height: u32) -> Option<DirtyRect> {
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
pub(crate) fn coverage(dab: &RetouchDab, x: u32, y: u32) -> Option<f32> {
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
    let rect = footprint(dab.pos, dab.radius, buf.width, buf.height)?;

    // The sample region: the footprint grown by the blur half-width and
    // the tilt, clamped to the image. A summed-area table over it gives
    // every window's average in O(1).
    let (w, h) = (i64::from(buf.width), i64::from(buf.height));
    let reach_x = blur + i64::from(tilt[0].unsigned_abs());
    let reach_y = blur + i64::from(tilt[1].unsigned_abs());
    let sx0 = (i64::from(rect.x0) - reach_x).max(0);
    let sy0 = (i64::from(rect.y0) - reach_y).max(0);
    let sx1 = (i64::from(rect.x1) + reach_x).min(w);
    let sy1 = (i64::from(rect.y1) + reach_y).min(h);
    let sat = SummedArea::build(buf, sx0, sy0, sx1, sy1);

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

    #[test]
    fn footprint_matches_the_shader_box() {
        // pos 2,2 r 6 → floor(-4) clamps to 0, ceil(8) → 8 inclusive.
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
}
