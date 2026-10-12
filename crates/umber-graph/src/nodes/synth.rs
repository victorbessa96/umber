//! Example-based texture synthesis: `synth` (requirements.md §4 P1 row,
//! "Example-based texture synthesis (EmbarkStudios/texture-synthesis
//! style) as first-class generator primitive"; the task brief filed it
//! under "§5:69" but line 69 of `docs/specs/requirements.md` sits in
//! §4's table, matching this row's text verbatim — treated as the
//! brief's mislabel, not a different row).
//!
//! # Algorithm (v1 scale, every constant stated)
//!
//! A 3-level multiresolution pyramid ([`PYRAMID_LEVELS`]), coarsest
//! first. Each level's output raster starts from an initial guess —
//! random draws from the example at level 0, a nearest-neighbor
//! upsample of the previous level's result at levels 1/2 — then every
//! texel is visited once in raster order and REPLACED by the
//! best-scoring candidate ([`refine_level`]): one raster pass per
//! level, not iterative relaxation (a deliberate v1 simplification of
//! Wei-Levoy-style multi-pass-per-level synthesis).
//!
//! Per pixel, the candidate pool ([`refine_level`]) is:
//! * up to 2 "coherence" candidates — the LEFT and UP neighbors' own
//!   chosen source positions, each stepped by the same raster offset
//!   `(+1, 0)` / `(0, +1)` ([`step_coherence`]) — propagating a
//!   contiguous patch from the example instead of a fresh random pick.
//! * [`K_RANDOM`] = 12 positions drawn uniformly at random (seeded,
//!   deterministic) from the level's valid example positions.
//!
//! Each candidate is scored by [`window_mse`]: the mean squared RGB
//! difference (alpha is never scored, only copied) over the
//! `(2*RADIUS+1)^2 - 1` = 24 texels surrounding the candidate (in the
//! example) and the output pixel (in the current raster), excluding the
//! center — [`RADIUS`] = 2, a 5x5 window. A coherence candidate's raw
//! score is discounted by the shipped [`COHERENCE_WEIGHT`] via
//! [`effective_score`]; the candidate with the lowest effective score
//! wins (strict first-index tie-break: coherence-left, then
//! coherence-up, then the random draws in generation order — ties never
//! fall to a later candidate). `randomness` widens this to a tolerance
//! band in mean-squared-difference units ([`RANDOMNESS_SCALE`]); at
//! `randomness = 0.0` selection is the plain deterministic argmin, no
//! RNG consumed.
//!
//! # Judgment calls (stated, per the brief's instruction)
//!
//! 1. **`map_mode` × `tiling_mode` instead of one 3-way enum.** The
//!    brief lists `map_mode` and `tiling_mode` as separate params but
//!    names three modes (Tiled / RandomWalk / Inpaint). Resolved as two
//!    orthogonal axes: `map_mode` (0 = RandomWalk, 1 = Inpaint) picks
//!    WHAT gets synthesized (the whole canvas vs. a masked hole);
//!    `tiling_mode` (0 = clamp, 1 = Tiled) picks HOW context sampling
//!    treats edges (clamped vs. toroidal, for both the output and the
//!    example). "Tiled" in the brief is `map_mode = 0, tiling_mode = 1`.
//!    `tiling_mode` is honored uniformly for Inpaint too (an unusual but
//!    not incoherent combination), rather than silently ignored.
//! 2. **Output size.** RandomWalk fills `ctx.resolution` (the generator
//!    convention noise/gradient already use). Inpaint's output is
//!    always the example's own native size — it is filling THAT image's
//!    hole, not generating a fresh canvas — so `ctx.resolution` is
//!    ignored for `map_mode = 1`, exactly like `image_asset` ignores it.
//! 3. **Inpaint uses the example as its own base.** There is no separate
//!    "base" param: the `example` asset IS the image being repaired, and
//!    `mask` (a wired `Image` input, luminance convention below) marks
//!    its hole. This matches the standard inpainting-via-texture-
//!    synthesis formulation and keeps the param list exactly as named in
//!    the brief (no extra "base" param invented).
//! 4. **Mask convention reused, not reinvented.** `mask` luminance
//!    `(r+g+b)/3 > 127`, tested integer-exact as `r+g+b > 381`
//!    ([`is_hole`]) — identical to [`crate::nodes::mask::FloodFillNode`]'s
//!    "inside" test. `true` = hole (synthesize); `false` = keep.
//! 5. **Alpha is excluded from matching.** [`window_mse`] compares R/G/B
//!    only (alpha is coverage, not texture content — the same reasoning
//!    `sharpen`/`curves` apply when they leave alpha untouched); the
//!    full 4-byte pixel (including alpha) is still copied verbatim on a
//!    write.
//! 6. **One raster pass per pyramid level**, not per-level relaxation
//!    sweeps. Keeps the v1 implementation's cost and behavior bounded
//!    and exactly specified; multi-pass relaxation is a natural wave-6
//!    quality knob, not required by the brief.
//! 7. **The coherence weight is a shipped constant, not a node param.**
//!    The brief's param list is exactly `example`, `map_mode`,
//!    `tiling_mode`, `randomness`, `seed` — [`COHERENCE_WEIGHT`] is
//!    internal, exposed only on the pure core ([`SynthConfig`]) so tests
//!    can vary it to prove the mechanism (see the coherence/wrap tests).
//! 8. **`randomness` clamps to `[0.0, 1.0]`** (non-finite is
//!    [`EvalError::BadParam`]; out-of-range silently clamps) — the same
//!    "clamp, don't error" convention `blur`'s negative radius uses,
//!    since "less than fully deterministic" and "more than maximally
//!    random" both have an obvious nearest-sane interpretation.
//! 9. **Coherence candidates omit rather than clamp at the example's
//!    edge under clamp (non-tiled) mode.** A clamped coherence step would
//!    silently alias onto a position that is not actually a
//!    continuation of the chosen patch; omitting it just means that
//!    pixel's pool loses one coherence candidate, never a wrong one.
//! 10. **The mask pyramid OR-pools toward coarser levels** (a coarse
//!     cell is valid/non-hole if ANY native pixel inside it is): this
//!     guarantees every level has a nonempty candidate pool whenever the
//!     native mask does (checked once, at eval time, for the finest
//!     level only — see [`SynthNode::eval`]), with no fallback logic or
//!     panics from an empty random-draw range.
//! 11. **The example's own color pyramid is box-filtered independently
//!     per level from the native image** (not cascaded level-to-level),
//!     avoiding compounding rounding; the finest level's box filter is a
//!     lossless identity (block size 1), so Inpaint's exact-copy
//!     guarantee never passes through a lossy resample.
//! 12. **Inpaint's exactness is a two-point stamp, not a lock.** Every
//!     pixel — hole or not — runs through the same raster refine pass
//!     (simpler, one code path for all three modes); non-hole pixels are
//!     then (re-)stamped to the example's exact bytes before AND after
//!     that pass (seeding good context for neighbors, then guaranteeing
//!     the output regardless of what refinement computed there).

use crate::value::ImageBuffer;
use crate::{EvalContext, EvalError, NodeImpl, NodeOutput, ParamValue};
use std::sync::Arc;

/// Neighborhood half-width: the scoring window spans `(2*RADIUS+1)^2`
/// texels (5x5 = 25) minus the center texel being decided — 24 context
/// samples, 3 channels (R/G/B) = 72 squared-difference terms averaged
/// per candidate by [`window_mse`].
const RADIUS: i32 = 2;

/// Random candidates drawn per pixel, in addition to up to 2 coherence
/// candidates (left/up propagation). Drawn with replacement.
const K_RANDOM: u32 = 12;

/// The shipped coherence bonus (not a node param — see the module docs'
/// judgment call 7 and [`SynthConfig`]). A coherence candidate's
/// effective score is `raw * (1.0 - COHERENCE_WEIGHT)`. At
/// `COHERENCE_WEIGHT = 1.0` a coherence candidate's effective score is
/// exactly `0.0` — the global minimum any sum-of-squares raw score can
/// tie but never beat — so it always wins the argmin's strict
/// first-index tie-break. This is the mechanism
/// [`tests::coherence_forces_contiguous_horizontal_sources`] and
/// [`tests::tiled_wrap_seam_is_exact_under_forced_coherence`] pin.
const COHERENCE_WEIGHT: f32 = 0.6;

/// `randomness`'s unit: the eligible-candidate band is
/// `best_effective_score + randomness * RANDOMNESS_SCALE`, in
/// mean-squared-channel-difference units. `255.0^2` is the largest
/// possible single-channel squared difference, so `randomness = 1.0`
/// admits candidates up to one full black/white channel flip's MSE
/// above the best (an additive band, not a multiplicative one — a
/// multiplicative band collapses to zero whenever the best score is
/// exactly 0.0, which is common with flat regions and exact matches).
const RANDOMNESS_SCALE: f32 = 65025.0;

/// Fixed pyramid depth (coarsest..finest): the v1 scale per the brief,
/// not a node param.
const PYRAMID_LEVELS: usize = 3;

/// A deterministic LCG stream (the same constants as
/// [`crate::nodes::noise::build_perm`]'s shuffle: `state = state *
/// 1664525 + 1013904223`), so the same seed always draws the same
/// sequence of candidates — the node's reproducibility guarantee.
struct Rng(u32);

impl Rng {
    fn new(seed: i32) -> Self {
        Self(seed as u32)
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        self.0
    }

    /// A uniform index in `0..n`. `n` must be nonzero (every call site
    /// draws from a validated-nonempty pool — see the module docs'
    /// judgment call 10).
    fn gen_range(&mut self, n: u32) -> u32 {
        self.next_u32() % n
    }
}

fn find_param<'a>(params: &'a [(String, ParamValue)], name: &str) -> Option<&'a ParamValue> {
    params.iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

fn int_param(params: &[(String, ParamValue)], name: &str, default: i32) -> Result<i32, EvalError> {
    match find_param(params, name) {
        None => Ok(default),
        Some(ParamValue::Int(v)) => Ok(*v),
        Some(_) => Err(EvalError::BadParam {
            node: 0,
            param: name.into(),
        }),
    }
}

fn float_param(
    params: &[(String, ParamValue)],
    name: &str,
    default: f32,
) -> Result<f32, EvalError> {
    let v = match find_param(params, name) {
        None => default,
        Some(ParamValue::Float(v)) => *v,
        Some(_) => {
            return Err(EvalError::BadParam {
                node: 0,
                param: name.into(),
            });
        }
    };
    if !v.is_finite() {
        return Err(EvalError::BadParam {
            node: 0,
            param: name.into(),
        });
    }
    Ok(v)
}

/// The node's `mask` input (required only for `map_mode = 1`/Inpaint).
fn named_image<'a>(
    inputs: &'a [(String, NodeOutput)],
    name: &str,
) -> Result<&'a ImageBuffer, EvalError> {
    match inputs.iter().find(|(n, _)| n == name) {
        None => Err(EvalError::MissingInput {
            node: 0,
            input: name.into(),
        }),
        Some((_, NodeOutput::Image(buf))) => Ok(buf),
        Some((_, other)) => Err(EvalError::TypeMismatch {
            node: 0,
            expected: "Image".into(),
            got: other.kind().into(),
        }),
    }
}

/// The mask convention shared with [`crate::nodes::mask::FloodFillNode`]:
/// luminance `(r+g+b)/3 > 127`, tested integer-exact as `r+g+b > 381`.
/// `true` = hole (to be synthesized); `false` = keep (byte-identical).
fn is_hole(px: [u8; 4]) -> bool {
    u32::from(px[0]) + u32::from(px[1]) + u32::from(px[2]) > 381
}

/// Rejects hostile raster sizes before allocation for `map_mode = 0`
/// (mirrors [`crate::nodes::noise`]'s `check_resolution`: each side
/// `<= 8192`, total texels `<= 2^26`).
fn check_resolution(width: u32, height: u32) -> Result<(), EvalError> {
    if width == 0 || height == 0 {
        return Err(EvalError::Image(crate::ImageError::EmptyDimensions {
            width,
            height,
        }));
    }
    if width > 8192 || height > 8192 || width as u64 * height as u64 > (1u64 << 26) {
        return Err(EvalError::Image(crate::ImageError::DimensionsTooLarge {
            width,
            height,
        }));
    }
    Ok(())
}

/// The 3 pyramid resolutions for a `(w, h)` target: `[coarsest, ...,
/// finest]`, finest `== (w, h)` exactly, each coarser level halving
/// (floored, `max(1)` guarded) independently of the level below it.
fn pyramid_dims(w: u32, h: u32) -> [(u32, u32); PYRAMID_LEVELS] {
    let finest = (w, h);
    let mid = ((finest.0 / 2).max(1), (finest.1 / 2).max(1));
    let coarse = ((mid.0 / 2).max(1), (mid.1 / 2).max(1));
    [coarse, mid, finest]
}

/// Box-filter downsample (or lossless identity when `out_w ==
/// src.width` and `out_h == src.height`) of `src` to `(out_w, out_h)`:
/// each output texel averages the source block `[x0, x1) x [y0, y1)`
/// per channel, round-to-nearest.
fn box_resample(src: &ImageBuffer, out_w: u32, out_h: u32) -> ImageBuffer {
    let (sw, sh) = (src.width, src.height);
    let mut data = Vec::with_capacity(out_w as usize * out_h as usize * 4);
    for oy in 0..out_h {
        let y0 = oy * sh / out_h;
        let y1 = ((oy + 1) * sh / out_h).max(y0 + 1).min(sh);
        for ox in 0..out_w {
            let x0 = ox * sw / out_w;
            let x1 = ((ox + 1) * sw / out_w).max(x0 + 1).min(sw);
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let px = src.pixel(x, y).expect("block coordinate stays in bounds");
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += u32::from(px[c]);
                    }
                    count += 1;
                }
            }
            let avg = |s: u32| ((s + count / 2) / count) as u8;
            data.extend_from_slice(&[avg(sum[0]), avg(sum[1]), avg(sum[2]), avg(sum[3])]);
        }
    }
    ImageBuffer::new(out_w, out_h, data).expect("dims match the loop bounds")
}

/// Nearest-neighbor resize of `src` to `(out_w, out_h)` — the pyramid's
/// level-to-level upsample for the OUTPUT raster (the example's own
/// color pyramid uses [`box_resample`] instead; see judgment call 11).
fn nn_resize_image(src: &ImageBuffer, out_w: u32, out_h: u32) -> ImageBuffer {
    let (sw, sh) = (src.width, src.height);
    let mut data = Vec::with_capacity(out_w as usize * out_h as usize * 4);
    for oy in 0..out_h {
        let sy = (oy * sh / out_h).min(sh - 1);
        for ox in 0..out_w {
            let sx = (ox * sw / out_w).min(sw - 1);
            let px = src
                .pixel(sx, sy)
                .expect("nn-resize coordinate stays in bounds");
            data.extend_from_slice(&px);
        }
    }
    ImageBuffer::new(out_w, out_h, data).expect("dims match the loop bounds")
}

/// Downsamples a hole-validity map (`true` = hole) from `(full_w,
/// full_h)` to `(out_w, out_h)` by OR-pooling: an output cell is a hole
/// only if EVERY native pixel inside its source block is a hole (see
/// judgment call 10 — this guarantees a nonempty valid set at every
/// level whenever the native mask has at least one non-hole pixel).
fn downsample_hole(full: &[bool], full_w: u32, full_h: u32, out_w: u32, out_h: u32) -> Vec<bool> {
    let mut out = vec![false; out_w as usize * out_h as usize];
    for oy in 0..out_h {
        let y0 = oy * full_h / out_h;
        let y1 = ((oy + 1) * full_h / out_h).max(y0 + 1).min(full_h);
        for ox in 0..out_w {
            let x0 = ox * full_w / out_w;
            let x1 = ((ox + 1) * full_w / out_w).max(x0 + 1).min(full_w);
            let mut any_valid = false;
            for y in y0..y1 {
                for x in x0..x1 {
                    if !full[(y * full_w + x) as usize] {
                        any_valid = true;
                    }
                }
            }
            out[(oy * out_w + ox) as usize] = !any_valid;
        }
    }
    out
}

/// Flattens a validity map (`true` = usable) into the list of positions
/// a candidate draw can pick from.
fn valid_positions(valid: &[bool], w: u32) -> Vec<(u32, u32)> {
    valid
        .iter()
        .enumerate()
        .filter(|&(_, v)| *v)
        .map(|(i, _)| ((i as u32) % w, (i as u32) / w))
        .collect()
}

/// Resolves one coordinate under the tiling edge rule: wraps modulo
/// `size` when `tiling`, else clamps to `[0, size - 1]`.
fn resolve_coord(v: i64, size: u32, tiling: bool) -> u32 {
    if tiling {
        v.rem_euclid(i64::from(size)) as u32
    } else {
        v.clamp(0, i64::from(size) - 1) as u32
    }
}

/// Mean squared RGB difference (alpha excluded — judgment call 5) over
/// the [`RADIUS`] window around `(ax, ay)` in `a` and `(bx, by)` in `b`,
/// excluding the center texel, edge rule per `tiling`.
fn window_mse(
    a: &ImageBuffer,
    ax: u32,
    ay: u32,
    b: &ImageBuffer,
    bx: u32,
    by: u32,
    tiling: bool,
) -> f32 {
    let mut sum = 0f32;
    let mut n = 0f32;
    for dy in -RADIUS..=RADIUS {
        for dx in -RADIUS..=RADIUS {
            if dx == 0 && dy == 0 {
                continue;
            }
            let apx = a
                .pixel(
                    resolve_coord(i64::from(ax) + i64::from(dx), a.width, tiling),
                    resolve_coord(i64::from(ay) + i64::from(dy), a.height, tiling),
                )
                .expect("resolve_coord stays in bounds");
            let bpx = b
                .pixel(
                    resolve_coord(i64::from(bx) + i64::from(dx), b.width, tiling),
                    resolve_coord(i64::from(by) + i64::from(dy), b.height, tiling),
                )
                .expect("resolve_coord stays in bounds");
            for c in 0..3 {
                let diff = f32::from(apx[c]) - f32::from(bpx[c]);
                sum += diff * diff;
            }
            n += 3.0;
        }
    }
    sum / n
}

/// Combines a candidate's raw [`window_mse`] with the coherence bonus
/// (see judgment call 7 and [`COHERENCE_WEIGHT`]'s doc comment). Lower
/// is better; non-coherence candidates are returned unchanged.
fn effective_score(raw: f32, is_coherence: bool, coherence_weight: f32) -> f32 {
    if is_coherence {
        raw * (1.0 - coherence_weight)
    } else {
        raw
    }
}

/// The coherence candidate stepped from a neighbor's chosen source by
/// the raster-adjacency offset `(dx, dy)` (`(1, 0)` for the left
/// neighbor, `(0, 1)` for the up neighbor). Wraps modulo the example
/// dimensions under `tiling`; under clamp mode the candidate is omitted
/// (`None`) rather than clamped onto a position that would not actually
/// continue the chosen patch (judgment call 9). Also omitted when the
/// stepped position is invalid (an Inpaint hole) per `valid`.
/// The example-space walking context `step_coherence` needs, bundled to
/// stay under clippy's 7-argument floor (judgment call 12).
struct CoherenceCtx<'a> {
    ex_w: u32,
    ex_h: u32,
    tiling: bool,
    valid: Option<&'a [bool]>,
}

fn step_coherence(
    sx: u32,
    sy: u32,
    dx: i64,
    dy: i64,
    ctx: &CoherenceCtx<'_>,
) -> Option<(u32, u32)> {
    let (nx, ny) = (i64::from(sx) + dx, i64::from(sy) + dy);
    let (rx, ry) = if ctx.tiling {
        (
            nx.rem_euclid(i64::from(ctx.ex_w)),
            ny.rem_euclid(i64::from(ctx.ex_h)),
        )
    } else {
        if nx < 0 || nx >= i64::from(ctx.ex_w) || ny < 0 || ny >= i64::from(ctx.ex_h) {
            return None;
        }
        (nx, ny)
    };
    let (rx, ry) = (rx as u32, ry as u32);
    if let Some(v) = ctx.valid {
        if !v[(ry * ctx.ex_w + rx) as usize] {
            return None;
        }
    }
    Some((rx, ry))
}

/// One pyramid level's example data: its box-resampled color buffer
/// plus (for Inpaint only) its OR-pooled validity map and the flattened
/// list of positions a candidate draw can pick from.
struct ExampleLevel {
    buf: ImageBuffer,
    valid_bool: Option<Vec<bool>>,
    valid_list: Option<Vec<(u32, u32)>>,
}

/// The synthesis run's tunables (see the module docs' judgment calls 1
/// and 7 — `coherence_weight` is exposed here, not as a node param, so
/// tests can vary it independently of [`SynthNode`]'s shipped constant).
#[derive(Debug, Clone, Copy)]
struct SynthConfig {
    /// `tiling_mode = 1`: wrap context sampling (output and example)
    /// instead of clamping.
    tiling: bool,
    /// Pre-clamped to `[0.0, 1.0]` by the caller.
    randomness: f32,
    seed: i32,
    /// Pre-clamped to `[0.0, 1.0]` by the caller.
    coherence_weight: f32,
}

/// Builds a level's initial raster by drawing, for every output texel,
/// an independent random valid position from the level's example.
fn init_random(level: &ExampleLevel, w: u32, h: u32, rng: &mut Rng) -> ImageBuffer {
    let (ew, eh) = (level.buf.width, level.buf.height);
    let mut data = Vec::with_capacity(w as usize * h as usize * 4);
    for _ in 0..(w as usize * h as usize) {
        let (sx, sy) = match &level.valid_list {
            Some(list) => list[rng.gen_range(list.len() as u32) as usize],
            None => (rng.gen_range(ew), rng.gen_range(eh)),
        };
        let px = level
            .buf
            .pixel(sx, sy)
            .expect("valid position stays in bounds");
        data.extend_from_slice(&px);
    }
    ImageBuffer::new(w, h, data).expect("dims match the loop bounds")
}

/// Refines every texel of `output` once, in raster order, against
/// `example` (see the module docs' algorithm section for the full
/// candidate-generation and selection rules). Returns the chosen source
/// position per texel, row-major (used for coherence propagation to
/// later pixels in the SAME pass, and returned by [`synthesize_core`]
/// for the seam/continuity tests).
fn refine_level(
    output: &mut ImageBuffer,
    example: &ImageBuffer,
    valid_bool: Option<&[bool]>,
    valid_list: Option<&[(u32, u32)]>,
    cfg: &SynthConfig,
    rng: &mut Rng,
) -> Vec<(u32, u32)> {
    let (w, h) = (output.width, output.height);
    let (ex_w, ex_h) = (example.width, example.height);
    let mut source = vec![(0u32, 0u32); w as usize * h as usize];
    let mut candidates: Vec<(u32, u32, bool)> = Vec::with_capacity(2 + K_RANDOM as usize);
    let mut effs: Vec<f32> = Vec::with_capacity(2 + K_RANDOM as usize);

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            candidates.clear();
            effs.clear();

            if x > 0 {
                let (lx, ly) = source[idx - 1];
                if let Some(c) = step_coherence(
                    lx,
                    ly,
                    1,
                    0,
                    &CoherenceCtx {
                        ex_w,
                        ex_h,
                        tiling: cfg.tiling,
                        valid: valid_bool,
                    },
                ) {
                    candidates.push((c.0, c.1, true));
                }
            }
            if y > 0 {
                let (ux, uy) = source[idx - w as usize];
                if let Some(c) = step_coherence(
                    ux,
                    uy,
                    0,
                    1,
                    &CoherenceCtx {
                        ex_w,
                        ex_h,
                        tiling: cfg.tiling,
                        valid: valid_bool,
                    },
                ) {
                    candidates.push((c.0, c.1, true));
                }
            }
            for _ in 0..K_RANDOM {
                let pos = match valid_list {
                    Some(list) => list[rng.gen_range(list.len() as u32) as usize],
                    None => (rng.gen_range(ex_w), rng.gen_range(ex_h)),
                };
                candidates.push((pos.0, pos.1, false));
            }

            for &(cx, cy, is_coh) in &candidates {
                let raw = window_mse(example, cx, cy, output, x, y, cfg.tiling);
                effs.push(effective_score(raw, is_coh, cfg.coherence_weight));
            }

            let mut best_i = 0usize;
            for i in 1..effs.len() {
                if effs[i] < effs[best_i] {
                    best_i = i;
                }
            }
            let chosen = if cfg.randomness > 0.0 {
                let threshold = effs[best_i] + cfg.randomness * RANDOMNESS_SCALE;
                let eligible: Vec<usize> =
                    (0..effs.len()).filter(|&i| effs[i] <= threshold).collect();
                eligible[rng.gen_range(eligible.len() as u32) as usize]
            } else {
                best_i
            };

            let (sx, sy, _) = candidates[chosen];
            source[idx] = (sx, sy);
            let px = example.pixel(sx, sy).expect("candidate stays in bounds");
            output.set_pixel(x, y, px);
        }
    }
    source
}

/// Runs the full pyramid synthesis. `hole` is `Some(native-resolution
/// validity, true = hole)` for Inpaint — in which case `(out_w, out_h)`
/// MUST equal `example`'s own dimensions (the caller enforces this; see
/// [`SynthNode::eval`]) — or `None` for RandomWalk/Tiled, where
/// `(out_w, out_h)` is the free target size. Returns the synthesized
/// image plus its finest-level source map (row-major, one example
/// `(x, y)` per output texel — overwritten to self-identity at Inpaint's
/// stamped/unmasked positions).
fn synthesize_core(
    example: &ImageBuffer,
    hole: Option<&[bool]>,
    out_w: u32,
    out_h: u32,
    cfg: &SynthConfig,
) -> (ImageBuffer, Vec<(u32, u32)>) {
    if hole.is_some() {
        debug_assert_eq!((out_w, out_h), (example.width, example.height));
    }
    let mut rng = Rng::new(cfg.seed);
    let out_dims = pyramid_dims(out_w, out_h);
    let ex_dims = pyramid_dims(example.width, example.height);

    let mut levels: Vec<ExampleLevel> = Vec::with_capacity(PYRAMID_LEVELS);
    for &(w, h) in &ex_dims {
        let buf = box_resample(example, w, h);
        let valid_bool = hole.map(|full| {
            downsample_hole(full, example.width, example.height, w, h)
                .into_iter()
                .map(|is_hole| !is_hole)
                .collect::<Vec<bool>>()
        });
        let valid_list = valid_bool.as_ref().map(|vb| valid_positions(vb, w));
        levels.push(ExampleLevel {
            buf,
            valid_bool,
            valid_list,
        });
    }

    let mut current: Option<ImageBuffer> = None;
    let mut source_map = Vec::new();
    for (l, level) in levels.iter().enumerate() {
        let (ow, oh) = out_dims[l];
        let mut buf = match &current {
            None => init_random(level, ow, oh, &mut rng),
            Some(prev) => nn_resize_image(prev, ow, oh),
        };
        let is_finest = l == PYRAMID_LEVELS - 1;
        if is_finest {
            if let Some(full_hole) = hole {
                for y in 0..oh {
                    for x in 0..ow {
                        if !full_hole[(y * ow + x) as usize] {
                            let px = example.pixel(x, y).expect("hole mask matches example size");
                            buf.set_pixel(x, y, px);
                        }
                    }
                }
            }
        }
        let src = refine_level(
            &mut buf,
            &level.buf,
            level.valid_bool.as_deref(),
            level.valid_list.as_deref(),
            cfg,
            &mut rng,
        );
        if is_finest {
            source_map = src;
            if let Some(full_hole) = hole {
                for y in 0..oh {
                    for x in 0..ow {
                        let idx = (y * ow + x) as usize;
                        if !full_hole[idx] {
                            let px = example.pixel(x, y).expect("hole mask matches example size");
                            buf.set_pixel(x, y, px);
                            source_map[idx] = (x, y);
                        }
                    }
                }
            }
        }
        current = Some(buf);
    }
    (current.expect("PYRAMID_LEVELS >= 1"), source_map)
}

/// `synth`: example-based texture synthesis. Params: `example: Asset`
/// (required, a filesystem PNG path — the same content-hash-path
/// convention [`crate::eval::NodeRegistry::seeded`]'s `image_asset` node
/// uses), `map_mode: Int` (default 0; 0 = RandomWalk, 1 = Inpaint),
/// `tiling_mode: Int` (default 0; 0 = clamp, 1 = Tiled/wrap-sampled),
/// `randomness: Float` (default 0.0, clamped `[0, 1]`), `seed: Int`
/// (default 0). Input `mask: Image`, required only for `map_mode = 1`
/// (see the module docs' judgment calls). Output: `Image` at
/// `ctx.resolution` for `map_mode = 0`, or at `example`'s own size for
/// `map_mode = 1`.
pub struct SynthNode;

impl NodeImpl for SynthNode {
    fn eval(
        &self,
        inputs: Vec<(String, NodeOutput)>,
        params: &[(String, ParamValue)],
        ctx: &EvalContext,
    ) -> Result<NodeOutput, EvalError> {
        let path = match find_param(params, "example") {
            Some(ParamValue::Asset(p)) => p,
            _ => {
                return Err(EvalError::BadParam {
                    node: 0,
                    param: "example".into(),
                });
            }
        };
        let map_mode = int_param(params, "map_mode", 0)?;
        if !(0..=1).contains(&map_mode) {
            return Err(EvalError::BadParam {
                node: 0,
                param: "map_mode".into(),
            });
        }
        let tiling_mode = int_param(params, "tiling_mode", 0)?;
        if !(0..=1).contains(&tiling_mode) {
            return Err(EvalError::BadParam {
                node: 0,
                param: "tiling_mode".into(),
            });
        }
        let randomness = float_param(params, "randomness", 0.0)?.clamp(0.0, 1.0);
        let seed = int_param(params, "seed", 0)?;

        let bytes = std::fs::read(path)?;
        let example = ImageBuffer::from_png_bytes(&bytes)?;

        let cfg = SynthConfig {
            tiling: tiling_mode == 1,
            randomness,
            seed,
            coherence_weight: COHERENCE_WEIGHT,
        };

        if map_mode == 1 {
            let mask = named_image(&inputs, "mask")?;
            if mask.width != example.width || mask.height != example.height {
                return Err(EvalError::BadParam {
                    node: 0,
                    param: "mask (size must match example)".into(),
                });
            }
            let hole: Vec<bool> = mask
                .data
                .chunks_exact(4)
                .map(|px| is_hole([px[0], px[1], px[2], px[3]]))
                .collect();
            if hole.iter().all(|&h| h) {
                return Err(EvalError::BadParam {
                    node: 0,
                    param: "mask (fully masked — no source pixels to sample)".into(),
                });
            }
            let (out, _source) =
                synthesize_core(&example, Some(&hole), example.width, example.height, &cfg);
            Ok(NodeOutput::Image(out))
        } else {
            let (w, h) = ctx.resolution;
            check_resolution(w, h)?;
            let (out, _source) = synthesize_core(&example, None, w, h, &cfg);
            Ok(NodeOutput::Image(out))
        }
    }
}

/// Registers `synth`.
pub fn register_synth_nodes(registry: &mut crate::NodeRegistry) {
    registry.register("synth", Arc::new(SynthNode));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A non-constant, deterministic fixture whose blue channel never
    /// reaches 255 (`% 255`, not `% 256`) — so no native pixel can ever
    /// equal the inpaint tests' `[255, 0, 255, 255]` sentinel, by
    /// construction, regardless of which candidate gets picked.
    fn varied_example(w: u32, h: u32) -> ImageBuffer {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let r = ((x * 37 + y * 101 + 13) % 256) as u8;
                let g = ((x * 61 + y * 17 + 5) % 256) as u8;
                let b = ((x * 19 + y * 83 + 3) % 255) as u8;
                data.extend_from_slice(&[r, g, b, 255]);
            }
        }
        ImageBuffer::new(w, h, data).unwrap()
    }

    /// Rows are solid colors (varies only in `y`) — every pair of
    /// horizontally adjacent pixels, including the wrap pair, is
    /// IDENTICAL by construction, so a forced horizontal propagation
    /// (see the coherence tests) makes the wrap seam exact, not merely
    /// tolerant.
    fn striped_example(w: u32, h: u32) -> ImageBuffer {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let v = ((y * 53 + 7) % 256) as u8;
            for _ in 0..w {
                data.extend_from_slice(&[v, v, v, 255]);
            }
        }
        ImageBuffer::new(w, h, data).unwrap()
    }

    #[test]
    fn effective_score_applies_the_shipped_coherence_bonus() {
        assert_eq!(effective_score(100.0, false, 0.6), 100.0);
        // 0.6 is inexact in binary f32 (1.0 - 0.6 = 0.39999998), so the
        // scaled score is 39.999996 — one ulp off 40.0 (f32 ulp at 40 is
        // ~3.8e-6). The shipped semantics are the multiplicative scale,
        // asserted to tolerance (crate convention, topo.rs), not to
        // exact equality.
        assert!((effective_score(100.0, true, 0.6) - 40.0).abs() < 1e-5);
        assert_eq!(effective_score(100.0, true, 1.0), 0.0);
        assert_eq!(effective_score(100.0, true, 0.0), 100.0);
    }

    #[test]
    fn same_seed_is_byte_identical_different_seed_differs() {
        let example = varied_example(10, 8);
        let cfg = |seed: i32| SynthConfig {
            tiling: false,
            randomness: 0.3,
            seed,
            coherence_weight: COHERENCE_WEIGHT,
        };
        let (a, _) = synthesize_core(&example, None, 12, 9, &cfg(7));
        let (b, _) = synthesize_core(&example, None, 12, 9, &cfg(7));
        assert_eq!(a.data, b.data, "same seed must be byte-identical");
        let (c, _) = synthesize_core(&example, None, 12, 9, &cfg(8));
        assert_ne!(a.data, c.data, "different seeds must differ");
    }

    #[test]
    fn coherence_forces_contiguous_horizontal_sources() {
        // weight = 1.0 makes a coherence candidate's effective score
        // exactly 0.0, the global minimum any raw SSD can tie but never
        // beat, so the strict first-index tie-break (coherence-left is
        // generated before the random draws) picks it for every x > 0,
        // regardless of what the random pool offers. Tiling keeps the
        // left step always in-bounds (never omitted at the example's
        // right edge), so the property holds across the whole row.
        let example = varied_example(6, 5);
        let cfg = SynthConfig {
            tiling: true,
            randomness: 0.0,
            seed: 42,
            coherence_weight: 1.0,
        };
        let (_out, source) = synthesize_core(&example, None, 10, 7, &cfg);
        let ex_w = example.width;
        for y in 0..7u32 {
            for x in 1..10u32 {
                let idx = (y * 10 + x) as usize;
                let (px, py) = source[idx - 1];
                let expected = ((px + 1) % ex_w, py);
                assert_eq!(
                    source[idx], expected,
                    "pixel ({x},{y}) must continue its left neighbor's source"
                );
            }
        }
    }

    #[test]
    fn tiled_wrap_seam_is_exact_under_forced_coherence() {
        // Example rows are solid colors. Column 0's and the last
        // column's chosen source positions are adjacent mod the example
        // width (forced by the coherence property above), and since a
        // row's color never varies with column, both read the SAME
        // color: an exact, not merely tolerant, wrap seam.
        let example = striped_example(9, 6);
        let cfg = SynthConfig {
            tiling: true,
            randomness: 0.0,
            seed: 11,
            coherence_weight: 1.0,
        };
        let (out, _source) = synthesize_core(&example, None, 9, 6, &cfg);
        for y in 0..6u32 {
            let left = out.pixel(0, y).expect("in bounds");
            let right = out.pixel(8, y).expect("in bounds");
            assert_eq!(left, right, "row {y}: wrap seam must be exact");
        }
    }

    #[test]
    fn inpaint_core_preserves_unmasked_bytes_and_excludes_the_hole_sentinel() {
        const SENTINEL: [u8; 4] = [255, 0, 255, 255];
        let (w, h) = (10, 8);
        let mut example = varied_example(w, h);
        let mut hole = vec![false; (w * h) as usize];
        for y in 2..5u32 {
            for x in 3..6u32 {
                example.set_pixel(x, y, SENTINEL);
                hole[(y * w + x) as usize] = true;
            }
        }
        let cfg = SynthConfig {
            tiling: false,
            randomness: 0.0,
            seed: 5,
            coherence_weight: COHERENCE_WEIGHT,
        };
        let (out, _source) = synthesize_core(&example, Some(&hole), w, h, &cfg);
        assert_eq!((out.width, out.height), (w, h));
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                let got = out.pixel(x, y).expect("in bounds");
                if hole[idx] {
                    assert_ne!(
                        got, SENTINEL,
                        "hole pixel ({x},{y}) must not keep the sentinel"
                    );
                } else {
                    assert_eq!(
                        got,
                        example.pixel(x, y).unwrap(),
                        "unmasked pixel ({x},{y}) must be byte-identical"
                    );
                }
            }
        }
    }

    #[test]
    fn inpaint_node_round_trips_through_a_temp_png() {
        const SENTINEL: [u8; 4] = [255, 0, 255, 255];
        let (w, h) = (8, 6);
        let mut example = varied_example(w, h);
        let mut mask_data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let inside = (2..5).contains(&x) && (1..4).contains(&y);
                if inside {
                    example.set_pixel(x, y, SENTINEL);
                }
                let m = if inside { 255 } else { 0 };
                mask_data.extend_from_slice(&[m, m, m, 255]);
            }
        }
        let mask = ImageBuffer::new(w, h, mask_data).unwrap();

        let bytes = example.to_png_bytes().unwrap();
        let path =
            std::env::temp_dir().join(format!("umber-synth-test-{}.png", std::process::id()));
        std::fs::write(&path, &bytes).expect("temp png writes");

        let params = vec![
            (
                "example".into(),
                ParamValue::Asset(path.to_string_lossy().into_owned()),
            ),
            ("map_mode".into(), ParamValue::Int(1)),
            ("seed".into(), ParamValue::Int(3)),
        ];
        let imp = SynthNode;
        let out = imp.eval(
            vec![("mask".into(), NodeOutput::Image(mask))],
            &params,
            &EvalContext::new(4, 4),
        );
        let _ = std::fs::remove_file(&path);
        let buf = match out.expect("inpaint evaluates") {
            NodeOutput::Image(buf) => buf,
            other => panic!("expected an Image, got {other:?}"),
        };
        assert_eq!((buf.width, buf.height), (w, h));
        for y in 0..h {
            for x in 0..w {
                let inside = (2..5).contains(&x) && (1..4).contains(&y);
                let got = buf.pixel(x, y).unwrap();
                if inside {
                    assert_ne!(got, SENTINEL, "({x},{y}) must not keep the sentinel");
                } else {
                    assert_eq!(got, example.pixel(x, y).unwrap(), "({x},{y}) unmasked");
                }
            }
        }
    }

    #[test]
    fn synth_node_rejects_bad_params_and_wiring() {
        let imp = SynthNode;
        let ctx = EvalContext::new(4, 4);

        let err = imp.eval(vec![], &[], &ctx);
        assert!(
            matches!(err, Err(EvalError::BadParam { .. })),
            "missing example must fail, got {err:?}"
        );

        let bad_path = vec![(
            "example".into(),
            ParamValue::Asset("/no/such/umber-synth-fixture.png".into()),
        )];
        let err = imp.eval(vec![], &bad_path, &ctx);
        assert!(
            matches!(err, Err(EvalError::Io(_))),
            "missing file must fail, got {err:?}"
        );

        let example = varied_example(4, 4);
        let path =
            std::env::temp_dir().join(format!("umber-synth-badparam-{}.png", std::process::id()));
        std::fs::write(&path, example.to_png_bytes().unwrap()).unwrap();
        let path_param = ParamValue::Asset(path.to_string_lossy().into_owned());

        let err = imp.eval(
            vec![],
            &[
                ("example".into(), path_param.clone()),
                ("map_mode".into(), ParamValue::Float(0.0)),
            ],
            &ctx,
        );
        assert!(
            matches!(err, Err(EvalError::BadParam { .. })),
            "mistyped map_mode must fail, got {err:?}"
        );

        let err = imp.eval(
            vec![],
            &[
                ("example".into(), path_param.clone()),
                ("map_mode".into(), ParamValue::Int(9)),
            ],
            &ctx,
        );
        assert!(
            matches!(err, Err(EvalError::BadParam { .. })),
            "out-of-range map_mode must fail, got {err:?}"
        );

        let err = imp.eval(
            vec![],
            &[
                ("example".into(), path_param.clone()),
                ("map_mode".into(), ParamValue::Int(1)),
            ],
            &ctx,
        );
        assert!(
            matches!(err, Err(EvalError::MissingInput { .. })),
            "inpaint with no mask wired must fail, got {err:?}"
        );

        let wrong_mask = NodeOutput::Image(ImageBuffer::filled(2, 2, [0, 0, 0, 255]).unwrap());
        let err = imp.eval(
            vec![("mask".into(), wrong_mask)],
            &[
                ("example".into(), path_param.clone()),
                ("map_mode".into(), ParamValue::Int(1)),
            ],
            &ctx,
        );
        assert!(
            matches!(err, Err(EvalError::BadParam { .. })),
            "mismatched mask size must fail, got {err:?}"
        );

        let full_mask = NodeOutput::Image(ImageBuffer::filled(4, 4, [255, 255, 255, 255]).unwrap());
        let err = imp.eval(
            vec![("mask".into(), full_mask)],
            &[
                ("example".into(), path_param.clone()),
                ("map_mode".into(), ParamValue::Int(1)),
            ],
            &ctx,
        );
        assert!(
            matches!(err, Err(EvalError::BadParam { .. })),
            "fully-masked mask must fail, got {err:?}"
        );

        let _ = std::fs::remove_file(&path);
    }
}
