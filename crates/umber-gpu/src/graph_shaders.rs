//! WGSL compute shaders for GPU node-graph evaluation.
//!
//! One shader module ([`GRAPH_EVAL_SHADER`]) with one `@compute` entry
//! point per GPU-evaluatable node family. Every entry point shares a
//! single bind-group layout (see the layout contract below), so
//! [`crate::graph_eval::GraphGpuEvaluator`] caches one pipeline per
//! entry point behind one layout — the same "always bound, fallbacks for
//! unused slots" convention as [`crate::shaders::MESH_SHADER`]'s IBL set
//! (a caller never re-creates a layout at runtime; unbound image inputs
//! read a 1x1 fallback texture, the noise table reads a zeroed fallback
//! buffer when the node is not a noise generator).
//!
//! # Layout contract
//!
//! - Binding 0 (`params: GraphParams`): four `vec4<f32>` slots. The
//!   per-entry packing is owned by [`crate::graph_eval`] — each entry's
//!   doc comment below names the slots it reads; slots it ignores may
//!   hold anything (the Rust side zeroes them).
//! - Bindings 1–3 (`src0..src2: texture_2d<f32>`): read-only image
//!   inputs, sampled with `textureLoad` at integer texels (no sampler,
//!   no filtering — filtering would smear bytes across texels; the blur
//!   and warp entries do their own addressing explicitly).
//! - Binding 4 (`perm: array<u32>`): the 512-entry doubled permutation
//!   table for the noise entries ([`umber_graph::nodes::build_perm`],
//!   widened `u8` → `u32` on upload). Non-noise entries never read it.
//! - Binding 5 (`dst: texture_storage_2d<rgba8unorm, write>`): the
//!   entry's output. `write`-only is core WebGPU (the same reason
//!   [`crate::bake_shaders::AO_BAKE_SHADER`] uses `write`, not
//!   `read_write`), so — unlike [`crate::paint::PaintCompositor`] — the
//!   graph evaluator needs no adapter-specific device feature.
//! - Binding 6 (`dims: vec2<u32>`): the output extent; every entry
//!   dispatches `ceil(w/16) x ceil(h/16)` workgroups of 16x16 and bounds-
//!   checks `global_invocation_id` itself.
//!
//! # Precision contract (CPU is the reference)
//!
//! The CPU engine (`umber_graph::eval`) stays the byte-exact reference.
//! These entries mirror its formulas op-for-op (same UV conventions, same
//! rounding helpers), but three differences are inherent to the port and
//! documented per entry where they apply: (a) `textureLoad` normalizes
//! bytes through f32 (`b/255 ± 1ulp`), so a store round-trips bytes
//! exactly everywhere except pathological half-ulp landings; (b) libm
//! `cos`/`sin`/`atan2`/`pow` may differ 1ulp from the CPU's, so the
//! gradient/noise/levels entries can land ±1 LSB from the CPU on rare
//! texels; (c) the blur entry's intermediate is `Rgba8Unorm`, whose
//! store quantization replaces the CPU's integer `(sum + k/2) / k`
//! rounding — again ±1 LSB at most. None of these affect topology,
//! routing, or the dirty-set contract; agreement testing stays CPU-side
//! until a device-backed golden lands (see the `gpu` test below).

/// Entry point: `uniform` with a `color` param (solid fill).
pub const ENTRY_FILL_UNIFORM: &str = "fill_uniform";
/// Entry point: `noise_perlin`.
pub const ENTRY_NOISE_PERLIN: &str = "gen_noise_perlin";
/// Entry point: `noise_value`.
pub const ENTRY_NOISE_VALUE: &str = "gen_noise_value";
/// Entry point: `noise_worley`.
pub const ENTRY_NOISE_WORLEY: &str = "gen_noise_worley";
/// Entry point: `gradient`.
pub const ENTRY_GRADIENT: &str = "gen_gradient";
/// Entry point: `checkerboard`.
pub const ENTRY_CHECKERBOARD: &str = "gen_checkerboard";
/// Entry point: `dots`.
pub const ENTRY_DOTS: &str = "gen_dots";
/// Entry point: `brick_pattern`.
pub const ENTRY_BRICK: &str = "gen_brick";
/// Entry point: `invert`.
pub const ENTRY_INVERT: &str = "op_invert";
/// Entry point: `levels`.
pub const ENTRY_LEVELS: &str = "op_levels";
/// Entry point: `curves`.
pub const ENTRY_CURVES: &str = "op_curves";
/// Entry point: `mix`.
pub const ENTRY_MIX: &str = "op_mix";
/// Entry point: `color_correct`.
pub const ENTRY_COLOR_CORRECT: &str = "op_color_correct";
/// Entry point: `hsv_adjust`.
pub const ENTRY_HSV: &str = "op_hsv";
/// Entry point: `triplanar_blend`.
pub const ENTRY_TRIPLANAR: &str = "op_triplanar";
/// Entry point: `edge_detect`.
pub const ENTRY_EDGE_DETECT: &str = "op_edge";
/// Entry point: `direction_warp`.
pub const ENTRY_WARP: &str = "op_warp";
/// Entry point: `blur`, horizontal separable pass.
pub const ENTRY_BLUR_H: &str = "op_blur_h";
/// Entry point: `blur`, vertical separable pass.
pub const ENTRY_BLUR_V: &str = "op_blur_v";
/// Entry point: `sharpen` (unsharp stage over a pre-blurred `src1`).
pub const ENTRY_SHARPEN: &str = "op_sharpen";

/// Every entry point in [`GRAPH_EVAL_SHADER`], in pipeline-cache order
/// (see [`crate::graph_eval::GpuOp::entry_points`], which must return
/// exactly these strings — the static test below pins the pairing).
pub const GRAPH_ENTRY_POINTS: &[&str] = &[
    ENTRY_FILL_UNIFORM,
    ENTRY_NOISE_PERLIN,
    ENTRY_NOISE_VALUE,
    ENTRY_NOISE_WORLEY,
    ENTRY_GRADIENT,
    ENTRY_CHECKERBOARD,
    ENTRY_DOTS,
    ENTRY_BRICK,
    ENTRY_INVERT,
    ENTRY_LEVELS,
    ENTRY_CURVES,
    ENTRY_MIX,
    ENTRY_COLOR_CORRECT,
    ENTRY_HSV,
    ENTRY_TRIPLANAR,
    ENTRY_EDGE_DETECT,
    ENTRY_WARP,
    ENTRY_BLUR_H,
    ENTRY_BLUR_V,
    ENTRY_SHARPEN,
];

/// GPU node-graph evaluation: per-family compute entries over one shared
/// bind-group layout (see the module docs for the layout + precision
/// contracts).
///
/// Per-entry param packing (`params.a` first — `b/c/d` only where named):
///
/// - `fill_uniform`: `a.xyz` = linear fill color, alpha forced to 1.
/// - `gen_noise_*`: `a = [scale, offset_x, offset_y, _]`; `perm` holds
///   the seed's doubled table (built CPU-side, same bytes as the CPU
///   engine). UV matches the CPU V-flip (`u = x/w*scale+ox`,
///   `v = (1-y/h)*scale+oz`, texel indices not centers).
/// - `gen_gradient`: `a = [kind, repeat, cx, cy]`, `b.x` = angle
///   degrees. Screen-space UV (`u = x/w`, `v = y/h`), same rotation and
///   `fract` ramp mapping as the CPU node.
/// - `gen_checkerboard`: `a.x` = scale (cells across).
/// - `gen_dots`: `a = [scale, radius, _, _]`; texel-center sampling with
///   the same 1px smoothstep AA band as the CPU node.
/// - `gen_brick`: `a = [sx, sy, mortar, _]`; running-bond offset +
///   mortar test mirror the CPU node; face/mortar colors are the exact
///   declared bytes normalized.
/// - `op_invert`: `255 - v` per RGB channel, alpha passthrough.
/// - `op_levels`: `a = [in_low, in_high, gamma, _]`,
///   `b = [out_low, out_high, _, _]`; gamma `1.0` skips `pow`
///   bit-exactly like the CPU fast path.
/// - `op_curves`: `a = [p0, p1]` (xyxy), `b = [p2, p3]`; clamped
///   piecewise-linear lerp, alpha passthrough.
/// - `op_mix`: `a.x` = factor; `src0` = fg, `src1` = bg (must match in
///   size — checked Rust-side); straight-alpha lerp incl. alpha.
/// - `op_color_correct`: `a = [hue_deg, saturation, brightness,
///   contrast]`; HSV stage skipped unless hue/saturation are live, same
///   fast-path structure as the CPU node.
/// - `op_hsv`: `a = [hue_deg, saturation, v_gain, _]`.
/// - `op_triplanar`: `a.xyz` = pre-normalized weights; `src0/1/2` =
///   x/y/z (identical sizes, checked Rust-side).
/// - `op_edge`: `a.x` = radius (1..=3, clamped Rust-side); dilated Sobel
///   on luminance, normalized by `1020√2` exactly like the CPU node.
/// - `op_warp`: `a = [off_x, off_y, strength, use_map]`; `src0` = input,
///   `src1` = vector map when `use_map > 0.5` (R/G remapped
///   `[0,255] → [-1,1]` times strength, overriding the uniform offset).
///   Forward warp with manual bilinear + edge clamp, mirroring the CPU
///   sampler (texel-index space, `round`-half-away via `floor(v+0.5)` —
///   WGSL's `round` is banker's rounding and would mismatch the CPU on
///   exact halves).
/// - `op_blur_h` / `op_blur_v`: `a.x` = radius; separable box average
///   with edge clamping and integer `(sum + k/2) / k` rounding in `u32`
///   (mirrors the CPU running-sum kernel, including alpha); radius 0
///   copies through.
/// - `op_sharpen`: `a.x` = amount; `src0` = source, `src1` = the blurred
///   source (produced by the blur entries); `out = src + amount *
///   (src - blur)` on RGB, alpha passthrough.
pub const GRAPH_EVAL_SHADER: &str = r#"
struct GraphParams {
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    d: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: GraphParams;
@group(0) @binding(1) var src0: texture_2d<f32>;
@group(0) @binding(2) var src1: texture_2d<f32>;
@group(0) @binding(3) var src2: texture_2d<f32>;
@group(0) @binding(4) var<storage, read> perm: array<u32>;
@group(0) @binding(5) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(6) var<uniform> dims: vec2<u32>;

/// Round-half-up float→byte step, returned normalized (mirrors the CPU
/// `byte_round`: `(clamp*255+0.5) as u8` truncates, i.e. floors, the
/// non-negative value — identical to `floor` here).
fn q8(v: f32) -> f32 {
    return floor(clamp(v, 0.0, 1.0) * 255.0 + 0.5) / 255.0;
}

fn in_bounds(gid: vec3<u32>) -> bool {
    return gid.x < dims.x && gid.y < dims.y;
}

fn coord_of(gid: vec3<u32>) -> vec2<i32> {
    return vec2<i32>(gid.xy);
}

/// Quintic fade (`6t^5 - 15t^4 + 10t^3`) for Perlin interpolation.
fn fade5(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

/// Smoothstep (`3t^2 - 2t^3`) for value-noise interpolation.
fn smooth3(t: f32) -> f32 {
    return t * t * (3.0 - 2.0 * t);
}

/// 2D gradient dot product: `h & 7` over the same 8 gradients as the CPU
/// node (diagonals pre-scaled by 1/sqrt(2)).
fn grad2(h: u32, x: f32, y: f32) -> f32 {
    let k = h & 7u;
    if (k == 0u) { return (x + y) * 0.7071067811865475; }
    if (k == 1u) { return (-x + y) * 0.7071067811865475; }
    if (k == 2u) { return (x - y) * 0.7071067811865475; }
    if (k == 3u) { return (-x - y) * 0.7071067811865475; }
    if (k == 4u) { return x; }
    if (k == 5u) { return -x; }
    if (k == 6u) { return y; }
    return -y;
}

/// Classic 2D Perlin noise in [-1, 1] over the bound `perm` table.
fn perlin2(x: f32, y: f32) -> f32 {
    let xi = i32(floor(x));
    let yi = i32(floor(y));
    let xf = x - floor(x);
    let yf = y - floor(y);
    let x0 = u32(xi & 255);
    let y0 = u32(yi & 255);
    let x1 = u32((xi + 1) & 255);
    let y1 = u32((yi + 1) & 255);
    let h00 = perm[x0 + perm[y0]];
    let h10 = perm[x1 + perm[y0]];
    let h01 = perm[x0 + perm[y1]];
    let h11 = perm[x1 + perm[y1]];
    let n00 = grad2(h00, xf, yf);
    let n10 = grad2(h10, xf - 1.0, yf);
    let n01 = grad2(h01, xf, yf - 1.0);
    let n11 = grad2(h11, xf - 1.0, yf - 1.0);
    let u = fade5(xf);
    let v = fade5(yf);
    return (n00 * (1.0 - u) + n10 * u) * (1.0 - v) + (n01 * (1.0 - u) + n11 * u) * v;
}

/// Lattice-corner hash in [0, 1] (the value-noise stream).
fn lattice_a(xi: i32, yi: i32) -> f32 {
    let x = u32(xi & 255);
    let y = u32(yi & 255);
    return f32(perm[(x + perm[y]) & 255u]) / 255.0;
}

/// Second hash axis for Worley feature points (offset selector, matching
/// the CPU node's +57/+131 stream split).
fn lattice_b(xi: i32, yi: i32) -> f32 {
    let x = u32((xi + 57) & 255);
    let y = u32((yi + 131) & 255);
    return f32(perm[(x + perm[y]) & 255u]) / 255.0;
}

/// Value noise in [0, 1]: hashed lattice corners, smoothstep blend.
fn value2(x: f32, y: f32) -> f32 {
    let xi = i32(floor(x));
    let yi = i32(floor(y));
    let xf = x - floor(x);
    let yf = y - floor(y);
    let v00 = lattice_a(xi, yi);
    let v10 = lattice_a(xi + 1, yi);
    let v01 = lattice_a(xi, yi + 1);
    let v11 = lattice_a(xi + 1, yi + 1);
    let u = smooth3(xf);
    let v = smooth3(yf);
    return (v00 * (1.0 - u) + v10 * u) * (1.0 - v) + (v01 * (1.0 - u) + v11 * u) * v;
}

/// Worley F1 in [0, 1]: nearest feature-point distance over the 3x3 cell
/// neighbourhood, clamped.
fn worley_f1(x: f32, y: f32) -> f32 {
    let cx = i32(floor(x));
    let cy = i32(floor(y));
    var best = 3.402823466e+38;
    for (var jy: i32 = -1; jy <= 1; jy++) {
        for (var jx: i32 = -1; jx <= 1; jx++) {
            let nx = cx + jx;
            let ny = cy + jy;
            let fx = f32(nx) + lattice_a(nx, ny);
            let fy = f32(ny) + lattice_b(nx, ny);
            let dx = x - fx;
            let dy = y - fy;
            let d = sqrt(dx * dx + dy * dy);
            best = min(best, d);
        }
    }
    return clamp(best, 0.0, 1.0);
}

/// Truncated remainder (Rust `%` semantics — WGSL has no float `%`).
/// The HSV entries need Rust's truncation (not floor) to agree with the
/// CPU's `rgb_to_hsv`/`hsv_to_rgb` on negative hues.
fn rmod(x: f32, y: f32) -> f32 {
    return x - trunc(x / y) * y;
}

/// Forward HSV map: `h` in degrees [0, 360), `s, v` in [0, 1].
fn rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let mx = max(max(c.r, c.g), c.b);
    let mn = min(min(c.r, c.g), c.b);
    let delta = mx - mn;
    var h = 0.0;
    // `mx == 0` implies `delta == 0` (all channels zero): saturation 0,
    // guarding the 0/0 the direct formula would produce.
    var s = 0.0;
    if (mx > 0.0) {
        s = delta / mx;
    }
    if (delta != 0.0) {
        if (mx == c.r) {
            h = 60.0 * rmod((c.g - c.b) / delta, 6.0);
        } else if (mx == c.g) {
            h = 60.0 * ((c.b - c.r) / delta + 2.0);
        } else {
            h = 60.0 * ((c.r - c.g) / delta + 4.0);
        }
    }
    if (h < 0.0) {
        h = h + 360.0;
    }
    return vec3<f32>(h, s, mx);
}

/// Inverse HSV map (sector-selected chroma + match, inputs clamped).
fn hsv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let s = clamp(c.y, 0.0, 1.0);
    let v = clamp(c.z, 0.0, 1.0);
    let h = rmod(rmod(c.x, 360.0) + 360.0, 360.0);
    let cc = v * s;
    let x = cc * (1.0 - abs(rmod(h / 60.0, 2.0) - 1.0));
    let m = v - cc;
    var rgb = vec3<f32>(0.0, 0.0, 0.0);
    if (h < 60.0) {
        rgb = vec3<f32>(cc, x, 0.0);
    } else if (h < 120.0) {
        rgb = vec3<f32>(x, cc, 0.0);
    } else if (h < 180.0) {
        rgb = vec3<f32>(0.0, cc, x);
    } else if (h < 240.0) {
        rgb = vec3<f32>(0.0, x, cc);
    } else if (h < 300.0) {
        rgb = vec3<f32>(x, 0.0, cc);
    } else {
        rgb = vec3<f32>(cc, 0.0, x);
    }
    return rgb + vec3<f32>(m, m, m);
}

/// One `levels` channel: remap + gamma + output range (params mirror the
/// CPU node's `in_low/in_high/gamma/out_low/out_high`).
fn levels_ch(v: f32) -> f32 {
    let in_low = params.a.x;
    let in_high = params.a.y;
    let gamma = params.a.z;
    let out_low = params.b.x;
    let out_high = params.b.y;
    let t = clamp((v - in_low) / (in_high - in_low), 0.0, 1.0);
    // `pow(0, 0)` is undefined in WGSL while the CPU defines it as 1
    // (gamma 0 flattens everything to `out_high`); the gamma-1 skip keeps
    // the identity path bit-exact like the CPU fast path.
    var g = t;
    if (gamma == 0.0) {
        g = 1.0;
    } else if (gamma != 1.0) {
        g = pow(t, gamma);
    }
    return q8(out_low + g * (out_high - out_low));
}

/// The 4-point control curve (`a = [p0, p1]` xyxy, `b = [p2, p3]`),
/// clamped piecewise-linear lerp mirroring the CPU `Curve4`.
fn curves_eval(x: f32) -> f32 {
    let p0 = params.a.xy;
    let p1 = params.a.zw;
    let p2 = params.b.xy;
    let p3 = params.b.zw;
    if (x <= p0.x) {
        return p0.y;
    }
    if (x >= p3.x) {
        return p3.y;
    }
    if (x <= p1.x) {
        return p0.y + (p1.y - p0.y) * ((x - p0.x) / (p1.x - p0.x));
    }
    if (x <= p2.x) {
        return p1.y + (p2.y - p1.y) * ((x - p1.x) / (p2.x - p1.x));
    }
    // `x < p3.x` here (checked above); the final segment. The trailing
    // `p3.y` is unreachable under the Rust-side strict-x validation, kept
    // so an unvalidated caller still terminates on a value.
    if (x <= p3.x) {
        return p2.y + (p3.y - p2.y) * ((x - p2.x) / (p3.x - p2.x));
    }
    return p3.y;
}

/// Luminance tap for the Sobel entry (edge-clamped, matching the CPU
/// node's border rule so flat fields read exactly zero gradient).
fn edge_lum(coord: vec2<i32>, dx: i32, dy: i32, w: u32, h: u32) -> f32 {
    let nx = clamp(coord.x + dx, 0, i32(w) - 1);
    let ny = clamp(coord.y + dy, 0, i32(h) - 1);
    let c = textureLoad(src0, vec2<i32>(nx, ny), 0);
    return (c.r + c.g + c.b) / 3.0;
}

/// Bilinear sample of `src0` in texel-index space with edge clamping
/// (mirrors the CPU warp sampler: corners at integer texel indices, the
/// fractional part blends them).
fn warp_sample(sx: f32, sy: f32, w: u32, h: u32) -> vec4<f32> {
    let x0 = i32(floor(sx));
    let y0 = i32(floor(sy));
    let fx = clamp(sx - f32(x0), 0.0, 1.0);
    let fy = clamp(sy - f32(y0), 0.0, 1.0);
    let xa = clamp(x0, 0, i32(w) - 1);
    let xb = clamp(x0 + 1, 0, i32(w) - 1);
    let ya = clamp(y0, 0, i32(h) - 1);
    let yb = clamp(y0 + 1, 0, i32(h) - 1);
    let p00 = textureLoad(src0, vec2<i32>(xa, ya), 0);
    let p10 = textureLoad(src0, vec2<i32>(xb, ya), 0);
    let p01 = textureLoad(src0, vec2<i32>(xa, yb), 0);
    let p11 = textureLoad(src0, vec2<i32>(xb, yb), 0);
    let top = mix(p00, p10, fx);
    let bot = mix(p01, p11, fx);
    return mix(top, bot, fy);
}

@compute @workgroup_size(16, 16)
fn fill_uniform(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    textureStore(dst, coord_of(gid), vec4<f32>(params.a.xyz, 1.0));
}

@compute @workgroup_size(16, 16)
fn gen_noise_perlin(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let w = f32(dims.x);
    let h = f32(dims.y);
    let scale = params.a.x;
    let u = f32(gid.x) / w * scale + params.a.y;
    let v = (1.0 - f32(gid.y) / h) * scale + params.a.z;
    let g = q8(perlin2(u, v) * 0.5 + 0.5);
    textureStore(dst, coord_of(gid), vec4<f32>(g, g, g, 1.0));
}

@compute @workgroup_size(16, 16)
fn gen_noise_value(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let w = f32(dims.x);
    let h = f32(dims.y);
    let scale = params.a.x;
    let u = f32(gid.x) / w * scale + params.a.y;
    let v = (1.0 - f32(gid.y) / h) * scale + params.a.z;
    let g = q8(value2(u, v));
    textureStore(dst, coord_of(gid), vec4<f32>(g, g, g, 1.0));
}

@compute @workgroup_size(16, 16)
fn gen_noise_worley(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let w = f32(dims.x);
    let h = f32(dims.y);
    let scale = params.a.x;
    let u = f32(gid.x) / w * scale + params.a.y;
    let v = (1.0 - f32(gid.y) / h) * scale + params.a.z;
    let g = q8(worley_f1(u, v));
    textureStore(dst, coord_of(gid), vec4<f32>(g, g, g, 1.0));
}

@compute @workgroup_size(16, 16)
fn gen_gradient(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let w = f32(dims.x);
    let h = f32(dims.y);
    let kind = u32(params.a.x);
    let rep = params.a.y;
    let cx = params.a.z;
    let cy = params.a.w;
    let u = f32(gid.x) / w;
    let v = f32(gid.y) / h;
    let px = u - cx;
    let py = v - cy;
    // CCW in screen space (y-down), mirroring the CPU rotation math.
    let rad = radians(params.b.x);
    let c = cos(rad);
    let s = sin(rad);
    let ruy = cy + (px * s + py * c);
    var t = 0.0;
    if (kind == 0u) {
        t = fract(ruy * rep);
    } else if (kind == 1u) {
        t = fract(length(vec2<f32>(px, py)) * rep);
    } else {
        // `atan2(0, 0)` is undefined in WGSL while the CPU defines it as
        // 0 — the exact-center texel takes the CPU value explicitly.
        var aa = atan2(py, px);
        if (px == 0.0 && py == 0.0) {
            aa = 0.0;
        }
        t = fract((aa / 6.283185307179586 + 0.5) * rep);
    }
    let g = q8(t);
    textureStore(dst, coord_of(gid), vec4<f32>(g, g, g, 1.0));
}

@compute @workgroup_size(16, 16)
fn gen_checkerboard(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let scale = params.a.x;
    let cx = i32(floor(f32(gid.x) * scale / f32(dims.x)));
    let cy = i32(floor(f32(gid.y) * scale / f32(dims.y)));
    // Non-negative by construction (scale > 0 is validated Rust-side),
    // so truncated `%` agrees with the CPU parity test.
    let white = (cx + cy) % 2 == 0;
    let g = select(0.0, 1.0, white);
    textureStore(dst, coord_of(gid), vec4<f32>(g, g, g, 1.0));
}

@compute @workgroup_size(16, 16)
fn gen_dots(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let scale = params.a.x;
    let radius = params.a.y;
    let w = f32(dims.x);
    let h = f32(dims.y);
    let cell_px = max(min(w / scale, h / scale), 1e-6);
    let half = 0.5 / cell_px;
    let gx = (f32(gid.x) + 0.5) * scale / w;
    let gy = (f32(gid.y) + 0.5) * scale / h;
    let lx = gx - floor(gx) - 0.5;
    let ly = gy - floor(gy) - 0.5;
    let d = sqrt(lx * lx + ly * ly);
    // `half > 0` always, so the smoothstep edges stay ordered (the
    // builtin is undefined for edge0 >= edge1).
    let cov = 1.0 - smoothstep(radius - half, radius + half, d);
    let g = q8(cov);
    textureStore(dst, coord_of(gid), vec4<f32>(g, g, g, 1.0));
}

@compute @workgroup_size(16, 16)
fn gen_brick(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let sx = params.a.x;
    let sy = params.a.y;
    let mortar = params.a.z;
    let w = f32(dims.x);
    let h = f32(dims.y);
    let inset = 0.5 - mortar / 2.0;
    let gy = f32(gid.y) * sy / h;
    let iy = i32(floor(gy));
    // Running bond: odd courses shift half a brick (non-negative `iy`,
    // so `%` matches the CPU test).
    let shift = select(0.0, 0.5, (iy % 2) != 0);
    let gx = f32(gid.x) * sx / w + shift;
    let fx = abs(gx - floor(gx) - 0.5);
    let fy = abs(gy - floor(gy) - 0.5);
    let is_mortar = fx >= inset || fy >= inset;
    // The exact declared face/mortar bytes, normalized.
    let face = vec3<f32>(180.0 / 255.0, 90.0 / 255.0, 70.0 / 255.0);
    let mort = vec3<f32>(64.0 / 255.0, 64.0 / 255.0, 64.0 / 255.0);
    let rgb = select(face, mort, is_mortar);
    textureStore(dst, coord_of(gid), vec4<f32>(rgb, 1.0));
}

@compute @workgroup_size(16, 16)
fn op_invert(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let c = textureLoad(src0, coord, 0);
    textureStore(dst, coord, vec4<f32>(1.0 - c.rgb, c.a));
}

@compute @workgroup_size(16, 16)
fn op_levels(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let c = textureLoad(src0, coord, 0);
    let rgb = vec3<f32>(levels_ch(c.r), levels_ch(c.g), levels_ch(c.b));
    textureStore(dst, coord, vec4<f32>(rgb, c.a));
}

@compute @workgroup_size(16, 16)
fn op_curves(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let c = textureLoad(src0, coord, 0);
    let rgb = vec3<f32>(q8(curves_eval(c.r)), q8(curves_eval(c.g)), q8(curves_eval(c.b)));
    textureStore(dst, coord, vec4<f32>(rgb, c.a));
}

@compute @workgroup_size(16, 16)
fn op_mix(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let fg = textureLoad(src0, coord, 0);
    let bg = textureLoad(src1, coord, 0);
    let f = params.a.x;
    let out = (1.0 - f) * bg + f * fg;
    textureStore(
        dst,
        coord,
        vec4<f32>(q8(out.r), q8(out.g), q8(out.b), q8(out.a)),
    );
}

@compute @workgroup_size(16, 16)
fn op_color_correct(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let c = textureLoad(src0, coord, 0);
    var rgb = c.rgb;
    let alpha = c.a;
    let hue = params.a.x;
    let saturation = params.a.y;
    let brightness = params.a.z;
    let contrast = params.a.w;
    // Same split fast paths as the CPU node: untouched stages stay
    // bit-exact.
    if (!(hue == 0.0 && saturation == 1.0)) {
        let hsv = rgb_to_hsv(rgb);
        rgb = hsv_to_rgb(vec3<f32>(hsv.x + hue, hsv.y * saturation, hsv.z));
    }
    if (!(brightness == 0.0 && contrast == 1.0)) {
        rgb = (rgb - vec3<f32>(0.5, 0.5, 0.5)) * contrast + vec3<f32>(0.5, 0.5, 0.5) + vec3<f32>(brightness);
    }
    textureStore(dst, coord, vec4<f32>(q8(rgb.r), q8(rgb.g), q8(rgb.b), alpha));
}

@compute @workgroup_size(16, 16)
fn op_hsv(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let c = textureLoad(src0, coord, 0);
    let hsv = rgb_to_hsv(c.rgb);
    let rgb = hsv_to_rgb(
        vec3<f32>(hsv.x + params.a.x, hsv.y * params.a.y, hsv.z * params.a.z),
    );
    textureStore(dst, coord, vec4<f32>(q8(rgb.r), q8(rgb.g), q8(rgb.b), c.a));
}

@compute @workgroup_size(16, 16)
fn op_triplanar(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let x = textureLoad(src0, coord, 0);
    let y = textureLoad(src1, coord, 0);
    let z = textureLoad(src2, coord, 0);
    // Weights arrive pre-normalized from the Rust side (zero-sum is a
    // Rust-side `BadParam`, never a silent divide here).
    let out = params.a.x * x + params.a.y * y + params.a.z * z;
    textureStore(
        dst,
        coord,
        vec4<f32>(q8(out.r), q8(out.g), q8(out.b), q8(out.a)),
    );
}

@compute @workgroup_size(16, 16)
fn op_edge(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let w = dims.x;
    let h = dims.y;
    let r = i32(params.a.x);
    let gx = (edge_lum(coord, r, -r, w, h) + 2.0 * edge_lum(coord, r, 0, w, h) + edge_lum(coord, r, r, w, h))
        - (edge_lum(coord, -r, -r, w, h) + 2.0 * edge_lum(coord, -r, 0, w, h) + edge_lum(coord, -r, r, w, h));
    let gy = (edge_lum(coord, -r, r, w, h) + 2.0 * edge_lum(coord, 0, r, w, h) + edge_lum(coord, r, r, w, h))
        - (edge_lum(coord, -r, -r, w, h) + 2.0 * edge_lum(coord, 0, -r, w, h) + edge_lum(coord, r, -r, w, h));
    let mag = sqrt(gx * gx + gy * gy);
    // Per-axis `|g| <= 1020` in byte space, so the max magnitude is
    // `1020√2` bytes = `4√2` in normalized units — the same normalization
    // as the CPU node's `SOBEL_MAX_MAG`.
    let m = q8(mag / 5.65685424949);
    textureStore(dst, coord, vec4<f32>(m, m, m, 1.0));
}

@compute @workgroup_size(16, 16)
fn op_warp(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let w = f32(dims.x);
    let h = f32(dims.y);
    var off = params.a.xy;
    if (params.a.w > 0.5) {
        let m = textureLoad(src1, coord, 0);
        off = (m.rg * 2.0 - vec2<f32>(1.0, 1.0)) * params.a.z;
    }
    // Forward warp: content shifts by +offset, so the sample sits at
    // `uv - offset` — in texel-index space exactly `(x - off.x*w, …)`,
    // mirroring the CPU sampler. The clamp keeps hostile offsets (finite
    // but far beyond any texture) inside `i32` range: WGSL float→int
    // conversion outside `i32` range is an indeterminate value, while the
    // CPU saturates — clamping closes that gap for every sane input
    // (nothing real sits past ±1M texels).
    let sx_raw = f32(gid.x) - off.x * w;
    let sy_raw = f32(gid.y) - off.y * h;
    let s = warp_sample(
        clamp(sx_raw, -1000000.0, 1000000.0),
        clamp(sy_raw, -1000000.0, 1000000.0),
        dims.x,
        dims.y,
    );
    textureStore(dst, coord, vec4<f32>(q8(s.r), q8(s.g), q8(s.b), q8(s.a)));
}

@compute @workgroup_size(16, 16)
fn op_blur_h(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let r = i32(params.a.x);
    if (r == 0) {
        textureStore(dst, coord, textureLoad(src0, coord, 0));
        return;
    }
    let k = u32(2 * r + 1);
    var sum = vec3<u32>(0u, 0u, 0u);
    var suma = 0u;
    for (var i: i32 = -r; i <= r; i++) {
        let nx = clamp(coord.x + i, 0, i32(dims.x) - 1);
        let c = textureLoad(src0, vec2<i32>(nx, coord.y), 0);
        // Back to byte space (`c*255` is within 1e-5 of the integer, so
        // the `+0.5` truncation recovers the exact byte): the `(sum +
        // k/2) / k` below is then the CPU's integer rounding, not a float
        // approximation of it.
        sum = sum + vec3<u32>(u32(c.r * 255.0 + 0.5), u32(c.g * 255.0 + 0.5), u32(c.b * 255.0 + 0.5));
        suma = suma + u32(c.a * 255.0 + 0.5);
    }
    let half = k / 2u;
    let q = (sum + vec3<u32>(half, half, half)) / vec3<u32>(k, k, k);
    let qa = (suma + half) / k;
    textureStore(dst, coord, vec4<f32>(vec3<f32>(q) / 255.0, f32(qa) / 255.0));
}

@compute @workgroup_size(16, 16)
fn op_blur_v(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let r = i32(params.a.x);
    if (r == 0) {
        textureStore(dst, coord, textureLoad(src0, coord, 0));
        return;
    }
    let k = u32(2 * r + 1);
    var sum = vec3<u32>(0u, 0u, 0u);
    var suma = 0u;
    for (var i: i32 = -r; i <= r; i++) {
        let ny = clamp(coord.y + i, 0, i32(dims.y) - 1);
        let c = textureLoad(src0, vec2<i32>(coord.x, ny), 0);
        sum = sum + vec3<u32>(u32(c.r * 255.0 + 0.5), u32(c.g * 255.0 + 0.5), u32(c.b * 255.0 + 0.5));
        suma = suma + u32(c.a * 255.0 + 0.5);
    }
    let half = k / 2u;
    let q = (sum + vec3<u32>(half, half, half)) / vec3<u32>(k, k, k);
    let qa = (suma + half) / k;
    textureStore(dst, coord, vec4<f32>(vec3<f32>(q) / 255.0, f32(qa) / 255.0));
}

@compute @workgroup_size(16, 16)
fn op_sharpen(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (!in_bounds(gid)) {
        return;
    }
    let coord = coord_of(gid);
    let s = textureLoad(src0, coord, 0);
    let b = textureLoad(src1, coord, 0);
    let amount = params.a.x;
    // Unsharp mask `out = src + amount * (src - blur)` on RGB; alpha
    // passes through (sharpening coverage would erode mask edges — the
    // CPU node's documented rule).
    let rgb = s.rgb + vec3<f32>(amount, amount, amount) * (s.rgb - b.rgb);
    textureStore(dst, coord, vec4<f32>(q8(rgb.r), q8(rgb.g), q8(rgb.b), s.a));
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_point_has_a_compute_definition() {
        // Each name in GRAPH_ENTRY_POINTS must have exactly one `fn <name>(`
        // definition. A typo in either the const or the WGSL breaks the
        // evaluator's pipeline cache at runtime — this pins the pairing
        // without a device.
        for entry in GRAPH_ENTRY_POINTS {
            let def = format!("fn {entry}(");
            assert_eq!(
                GRAPH_EVAL_SHADER.matches(def.as_str()).count(),
                1,
                "entry point {entry:?} must be defined exactly once"
            );
        }
        // No stray entry points: the `@compute` count equals the table.
        assert_eq!(
            GRAPH_EVAL_SHADER.matches("@compute").count(),
            GRAPH_ENTRY_POINTS.len(),
            "every @compute entry must be listed in GRAPH_ENTRY_POINTS"
        );
    }

    #[test]
    fn shared_layout_bindings_are_all_declared() {
        // The evaluator binds all seven slots on every dispatch (see the
        // module docs); a deleted binding would fail pipeline creation on
        // a device, so pin the declarations statically too.
        for binding in [
            "@binding(0) var<uniform> params: GraphParams",
            "@binding(1) var src0: texture_2d<f32>",
            "@binding(2) var src1: texture_2d<f32>",
            "@binding(3) var src2: texture_2d<f32>",
            "@binding(4) var<storage, read> perm: array<u32>",
            "@binding(5) var dst: texture_storage_2d<rgba8unorm, write>",
            "@binding(6) var<uniform> dims: vec2<u32>",
        ] {
            assert!(
                GRAPH_EVAL_SHADER.contains(binding),
                "shared layout must declare {binding:?}"
            );
        }
    }

    /// The `gpu`-gated parse check: `create_shader_module` runs the full
    /// naga WGSL frontend, so module creation failing means the shader
    /// does not parse. Plain default device on purpose — the graph pass
    /// uses `write`-only storage textures (core WebGPU), unlike the paint
    /// pass's `read_write` (see `paint::PaintError::MissingDeviceFeature`).
    #[cfg(feature = "gpu")]
    #[test]
    fn shader_module_parses_on_a_real_device() {
        let Some((device, _queue)) = try_request_device() else {
            return;
        };
        let _module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("umber_graph_eval_shader_parse_check"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(GRAPH_EVAL_SHADER)),
        });
    }

    #[cfg(feature = "gpu")]
    fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("skipping: no wgpu adapter available");
            return None;
        };
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }
}
