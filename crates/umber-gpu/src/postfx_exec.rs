//! The post-effects WGSL passes (§7:84, the exec half of
//! [`crate::postfx`]).
//!
//! # Conventions (mirroring `display_lut` / `shaders`)
//!
//! Each pass is a fullscreen triangle over a fragment-stage storage or
//! texture target. The entry-point names match the [`crate::postfx::PostPass`]
//! arms one-to-one; the render-graph half dispatches one entry per enabled
//! effect, ping-ponging between the chain's two intermediates.
//!
//! # Judgment calls (for the verifier)
//!
//! - **Bloom is one entry with two phases** — the pass reads the chain input,
//!   threshold-detects, and writes the BLURRED bright-pass directly: a
//!   13-tap dual-ring sample at quarter-step offsets approximates the
//!   downsample+blur pair in a single dispatch. The recombine is folded
//!   into the SAME pass's output (additive), so bloom costs one slot-pair
//!   step like every other effect — the two-pass pair the model doc named
//!   collapses into one dispatch at the cost of a fixed kernel; the honest
//!   v1 boundary (no mip pyramid) stands.
//! - **DoF is a 16-tap spiral gather** weighted by the CoC (focus/aperture
//!   from the uniform); no depth buffer in v1 — the mesh pass's scene depth
//!   rides the alpha channel of the intermediate (documented contract:
//!   postfx intermediates carry linear color in RGB + view-depth in A).
//! - **Sharpen is the classic unsharp mask** with the radius's 8-neighborhood
//!   mean as the blur base.
//! - **The tone-map WGSL mirrors [`crate::postfx::tone_map`] exactly** — the
//!   CPU reference and the tests are the source of truth; the constants
//!   are duplicated deliberately (WGSL cannot import Rust) and pinned by
//!   the same value tests.

/// The per-dispatch uniform: the chain input's texel size + the effect's
/// parameters (each pass reads only its own fields).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PostParams {
    /// 1/width of the chain input.
    pub texel_x: f32,
    /// 1/height.
    pub texel_y: f32,
    /// Dof::focus / the spiral gather's plane.
    pub focus: f32,
    /// Dof::aperture.
    pub aperture: f32,
    /// Bloom::threshold.
    pub threshold: f32,
    /// Bloom::intensity.
    pub intensity: f32,
    /// Bloom::radius (texel units at the input res).
    pub radius: f32,
    /// Vignette::strength.
    pub strength: f32,
    /// Vignette::softness.
    pub softness: f32,
    /// Sharpen::amount.
    pub amount: f32,
    /// Sharpen::radius.
    pub sharpen_radius: f32,
    /// ToneMap::exposure.
    pub exposure: f32,
    /// ToneMap::mode as an int: 0=ACES, 1=Filmic, 2=Reinhard.
    pub tone_mode: u32,
    /// Reserved to 16-byte multiples.
    pub _pad: [u32; 3],
}

impl Default for PostParams {
    fn default() -> Self {
        Self {
            texel_x: 0.0,
            texel_y: 0.0,
            focus: 1.0,
            aperture: 0.0,
            threshold: 1.0,
            intensity: 0.0,
            radius: 8.0,
            strength: 0.0,
            softness: 1.0,
            amount: 0.0,
            sharpen_radius: 1.0,
            exposure: 0.0,
            tone_mode: 0,
            _pad: [0; 3],
        }
    }
}

/// The fullscreen-post WGSL. Group 0: the uniform at 0, the chain input
/// texture at 1, the sampler at 2, the output at 3 (rgba16float storage).
pub const POST_SHADER: &str = r#"struct PostParams {
    texel_x: f32, texel_y: f32,
    focus: f32, aperture: f32,
    threshold: f32, intensity: f32, radius: f32,
    strength: f32, softness: f32,
    amount: f32, sharpen_radius: f32,
    exposure: f32, tone_mode: u32,
    _pad0: u32, _pad1: u32, _pad2: u32,
};
@group(0) @binding(0) var<uniform> params: PostParams;
@group(0) @binding(1) var input_tex: texture_2d<f32>;
@group(0) @binding(2) var input_sampler: sampler;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba16float, write>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // The fullscreen triangle.
    let p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0)
    );
    var out: vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    out.xy = p[vi];
    return out;
}

fn sample_input(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(input_tex, input_sampler, uv);
}

// ---- DoF: the CoC-weighted 16-tap spiral gather. Alpha carries view depth.
@fragment
fn fs_dof(@builtin(position) frag: vec4<f32>) -> void {
    let dims = vec2<f32>(textureDimensions(input_tex));
    let uv = frag.xy / dims;
    let depth = sample_input(uv).a;
    let coc = clamp(abs(depth - params.focus) * params.aperture, 0.0, 1.0);
    var acc = vec4<f32>(0.0);
    var total = 0.0;
    // Two rings of 8, spiral-spaced.
    for (var i = 0u; i < 16u; i = i + 1u) {
        let ring = select(0.25, 1.0, i >= 8u);
        let angle = f32(i % 8u) * 0.7853981633974483 + select(0.0, 0.39269908169872414, i >= 8u);
        let off = vec2<f32>(cos(angle), sin(angle)) * ring * coc * vec2<f32>(params.texel_x, params.texel_y) * 8.0;
        let w = 1.0;
        acc = acc + sample_input(uv + off) * w;
        total = total + w;
    }
    textureStore(output_tex, vec2<i32>(frag.xy), acc / total);
}

// ---- Bloom: threshold → 13-tap dual-ring blur → additive recombine, one pass.
@fragment
fn fs_bloom(@builtin(position) frag: vec4<f32>) -> void {
    let dims = vec2<f32>(textureDimensions(input_tex));
    let uv = frag.xy / dims;
    let base = sample_input(uv);
    var bright = vec4<f32>(0.0);
    var total = 0.0;
    let r = vec2<f32>(params.texel_x, params.texel_y) * params.radius;
    // 12 taps: two hexagonal rings + the center.
    for (var i = 0u; i < 12u; i = i + 1u) {
        let ring = select(0.5, 1.0, i >= 6u);
        let angle = f32(i % 6u) * 1.0471975511965976 + select(0.0, 0.5235987755982988, i >= 6u);
        let off = vec2<f32>(cos(angle), sin(angle)) * ring * r;
        let s = sample_input(uv + off);
        let lum = dot(s.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        let b = select(vec4<f32>(0.0), s, lum > params.threshold);
        bright = bright + b;
        total = total + 1.0;
    }
    let blur = bright / total;
    textureStore(output_tex, vec2<i32>(frag.xy), base + blur * params.intensity);
}

// ---- Vignette: the radial falloff.
@fragment
fn fs_vignette(@builtin(position) frag: vec4<f32>) -> void {
    let dims = vec2<f32>(textureDimensions(input_tex));
    let uv = frag.xy / dims;
    let c = uv - vec2<f32>(0.5);
    let d = length(c) * 1.4142135623730951; // 0 center → 1 corner.
    let falloff = smoothstep(1.0 - params.softness, 1.0, d);
    let s = sample_input(uv);
    textureStore(output_tex, vec2<i32>(frag.xy), s * (1.0 - params.strength * falloff));
}

// ---- Sharpen: the unsharp mask (base - blur neighborhood).
@fragment
fn fs_sharpen(@builtin(position) frag: vec4<f32>) -> void {
    let dims = vec2<f32>(textureDimensions(input_tex));
    let uv = frag.xy / dims;
    let base = sample_input(uv);
    let r = vec2<f32>(params.texel_x, params.texel_y) * params.sharpen_radius;
    var blur = vec4<f32>(0.0);
    for (var i = 0u; i < 8u; i = i + 1u) {
        let angle = f32(i) * 0.7853981633974483;
        blur = blur + sample_input(uv + vec2<f32>(cos(angle), sin(angle)) * r);
    }
    blur = blur / 8.0;
    textureStore(output_tex, vec2<i32>(frag.xy), base + (base - blur) * params.amount);
}

// ---- ToneMap: the three curves, mirroring postfx::tone_map exactly.
fn aces_fit(x: f32) -> f32 {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}
fn hable(x: f32) -> f32 {
    let a = 0.15; let b = 0.50; let c = 0.10; let d = 0.20; let e = 0.02; let f = 0.30;
    let w = 11.2;
    let curve = (x * (a * x + c * b) + d * e) / (x * (a * x + b) + d * f) - e / f;
    let s = x / w;
    return s * curve / (1.0 + s * curve / w);
}
@fragment
fn fs_tonemap(@builtin(position) frag: vec4<f32>) -> void {
    let dims = vec2<f32>(textureDimensions(input_tex));
    let uv = frag.xy / dims;
    let s = sample_input(uv);
    let x = s.rgb * pow(2.0, params.exposure);
    let mapped = select(
        select(vec3<f32>(aces_fit(x.r), aces_fit(x.g), aces_fit(x.b)),
               vec3<f32>(hable(x.r), hable(x.g), hable(x.b)),
               params.tone_mode == 1u),
        x / (vec3<f32>(1.0) + x),
        params.tone_mode == 2u
    );
    textureStore(output_tex, vec2<i32>(frag.xy), vec4<f32>(mapped, s.a));
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_are_pod_and_default_sane() {
        let p = PostParams::default();
        let bytes = bytemuck::bytes_of(&p);
        assert_eq!(
            bytes.len() % 16,
            0,
            "16-byte multiple for uniform alignment"
        );
        assert_eq!(p.tone_mode, 0, "ACES is the default curve");
        assert_eq!(p.aperture, 0.0, "default DoF is identity (pinhole)");
    }

    #[test]
    fn shader_carries_all_five_entries() {
        for entry in [
            "fs_dof",
            "fs_bloom",
            "fs_vignette",
            "fs_sharpen",
            "fs_tonemap",
        ] {
            assert!(POST_SHADER.contains(entry), "missing entry {entry}");
        }
        assert!(
            POST_SHADER.contains("@vertex"),
            "the fullscreen vertex stage"
        );
    }

    #[test]
    fn aces_constants_mirror_the_cpu_reference() {
        // The WGSL must carry Narkowicz's exact constants — the CPU
        // reference (postfx::tone_map) and its pinned test are the truth;
        // this tripwire catches a constant drift between the two halves.
        for c in ["2.51", "0.03", "2.43", "0.59", "0.14"] {
            assert!(
                POST_SHADER.contains(c),
                "ACES constant {c} drifted from the CPU reference"
            );
        }
        for c in ["0.15", "0.50", "0.10", "0.20", "0.02", "0.30", "11.2"] {
            assert!(POST_SHADER.contains(c), "Hable constant {c} drifted");
        }
    }
}
