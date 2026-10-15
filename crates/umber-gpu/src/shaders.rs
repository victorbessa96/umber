//! WGSL shader sources for the viewport mesh pass.
//!
//! Normal-shaded mesh pass with a directional light and a hemispherical
//! environment light: the procedural sky/ground irradiance by default,
//! or a convolved environment map when one is bound (Wave-4 item 6 —
//! see `crate::ibl`'s convention block for the equirect math shared
//! with the convolve shader) plus the v1 specular IBL tier (three
//! cone-scaled prefiltered levels × the analytic BRDF fit — see
//! `crate::ibl::IBL_PREFILTER_SHADER`). Vertex normals drive the
//! lighting; the albedo is the bound paint tile when one is set, else
//! the `BASE_COLOR` constant (core-loop kill 1 — the viewport shows
//! paint; see `paint_albedo` below for the fallback contract).
//! Matches the `Vertex` and `CameraUniform` layouts in `renderer.rs`;
//! bindings 1–3 are the diffuse IBL set, 4–7 the specular tier (see
//! `GpuContext`'s bind-group layout), binding 8 the view-mode
//! selector (`crate::view_mode::VIEW_MODE_WGSL`, verbatim), bindings
//! 9–11 the base-color paint set (`paint_tex`, `paint_sampler`,
//! `PaintParams` — group 0, not group 2, so every existing group-0
//! bind group and draw keeps working with the fallback bound; the
//! group-2 per-channel reservation in `crate::view_mode` stays the
//! Wave-2 contract for the full channel set). Binding 12 is the
//! height channel's texture (audit worklist #12, "height→normal live
//! in viewport"): reuses the paint sampler/addressing, gated by its
//! own `PaintParams` flag bit, folded into `shade`'s lit path as a
//! per-frame normal perturbation (no bake, no TBN — see
//! `height_perturbed_normal`'s doc for the v1 boundary). Group 1 is the display LUT
//! (`crate::display_lut`; the snippet is that module's
//! `DISPLAY_LUT_WGSL`, verbatim), applied to the final shaded color as
//! the last step before the target.
///
/// Two fragment entries (the LUT gate, see [`MESH_ENTRY_DIRECT`] /
/// [`MESH_ENTRY_SCENE_LINEAR`]): `fs_main` applies the LUT inline (the
/// direct path, unchanged), `fs_scene_linear` returns the same shaded
/// color without it (the post-effects path, `crate::postfx_viewport`).
/// Both call the one `shade` helper, so the two can never disagree on
/// anything but the LUT.
pub const MESH_SHADER: &str = r#"
// ---- display LUT (crate::display_lut — keep verbatim) ----
@group(1) @binding(0) var display_lut: texture_2d<f32>;

fn display_lut_index(c: f32) -> i32 {
    return i32(floor(clamp(c, 0.0, 1.0) * 255.0 + 0.5));
}

fn apply_display_lut(color: vec3<f32>) -> vec3<f32> {
    let r = textureLoad(display_lut, vec2<i32>(display_lut_index(color.r), 0), 0).r;
    let g = textureLoad(display_lut, vec2<i32>(display_lut_index(color.g), 0), 0).g;
    let b = textureLoad(display_lut, vec2<i32>(display_lut_index(color.b), 0), 0).b;
    return vec3<f32>(r, g, b);
}
// ---- end display LUT ----

struct Camera {
    view_proj: mat4x4<f32>,
    // xyz = normalized direction the light travels (surface -> fragment),
    // w unused (padding to satisfy uniform alignment).
    light_dir: vec4<f32>,
    // World-space eye position; unused here (this pass has no
    // view-dependent term) but kept so this struct's layout matches
    // `CameraUniform` exactly, as `renderer.rs` documents — `OPENPBR_SHADER`
    // reads this same trailing field for its specular/Fresnel terms.
    eye: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
// IBL set (Wave-4 item 6): the convolved 32x16 irradiance map, its
// sampler, and the procedural/map selector. All three are always bound
// (the context supplies a 1x1 fallback texture when no environment is
// loaded) so the pipeline layout never changes at runtime.
@group(0) @binding(1) var irradiance_tex: texture_2d<f32>;
@group(0) @binding(2) var irradiance_sampler: sampler;
struct EnvFlags {
    flags: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};
@group(0) @binding(3) var<uniform> env_flags: EnvFlags;
// Specular IBL tier (wave-5 v1): the three cone-scaled prefiltered
// levels (`crate::ibl::PREFILTER_ROUGHNESS`) plus the roughness/
// metallic uniform. The irradiance sampler (binding 2) is reused —
// the prefiltered maps share its repeat-U/clamp-V filtering. All four
// bindings are always populated (1x1 fallbacks when no environment is
// loaded) so the pipeline layout never changes at runtime.
@group(0) @binding(4) var prefilter0_tex: texture_2d<f32>;
@group(0) @binding(5) var prefilter1_tex: texture_2d<f32>;
@group(0) @binding(6) var prefilter2_tex: texture_2d<f32>;
struct SpecParams {
    roughness: f32,
    metallic: f32,
    _pad0: f32,
    _pad1: f32,
};
@group(0) @binding(7) var<uniform> spec_params: SpecParams;
// ---- view mode (crate::view_mode — keep verbatim) ----
struct ViewModeParams {
    mode: u32,
    channel: u32,
    _pad0: u32,
    _pad1: u32,
};
@group(0) @binding(8) var<uniform> view_mode: ViewModeParams;

const VIEW_MODE_LIT: u32 = 0u;
const VIEW_MODE_UNLIT: u32 = 1u;
const VIEW_MODE_SOLO: u32 = 2u;
const VIEW_MODE_MESH_PREVIEW: u32 = 3u;

const VIEW_MODE_SOLO_BASE_COLOR: u32 = 0u;
const VIEW_MODE_SOLO_EMISSIVE: u32 = 1u;
const VIEW_MODE_SOLO_ROUGHNESS: u32 = 2u;
const VIEW_MODE_SOLO_METALLIC: u32 = 3u;
const VIEW_MODE_SOLO_AO: u32 = 4u;
const VIEW_MODE_SOLO_HEIGHT: u32 = 5u;
const VIEW_MODE_SOLO_NORMAL: u32 = 6u;

const VIEW_MODE_PREVIEW_AO: u32 = 0u;
const VIEW_MODE_PREVIEW_CURVATURE: u32 = 1u;
const VIEW_MODE_PREVIEW_NORMAL_BASE: u32 = 2u;
const VIEW_MODE_PREVIEW_POSITION: u32 = 3u;
const VIEW_MODE_PREVIEW_THICKNESS: u32 = 4u;
const VIEW_MODE_PREVIEW_ID: u32 = 5u;
const VIEW_MODE_PREVIEW_HEIGHT: u32 = 6u;
const VIEW_MODE_PREVIEW_BENT_NORMAL: u32 = 7u;
const VIEW_MODE_PREVIEW_WORLD_SPACE_NORMAL: u32 = 8u;

fn view_mode_bypasses_lut(mode: u32, channel: u32) -> bool {
    return (mode == VIEW_MODE_SOLO && channel > VIEW_MODE_SOLO_EMISSIVE) || (mode == VIEW_MODE_MESH_PREVIEW);
}
// ---- end view mode ----

// ---- paint tile (core-loop kill 1 — the viewport shows paint) ----
// The composited base-color tile: one `PaintTarget`'s view (rgba8,
// filterable), a shared filtering clamp sampler, and the tile-select
// uniform. All three are always bound (a 1x1 fallback texture +
// `flags == 0` when no paint is set) so the pipeline layout never
// changes at runtime — the same always-populated pattern as the IBL
// set above. Appended AFTER the view-mode selector: additive bindings,
// the classic layout-sync trap avoided by construction.
@group(0) @binding(9) var paint_tex: texture_2d<f32>;
@group(0) @binding(10) var paint_sampler: sampler;
struct PaintParams {
    uv_offset: vec2<f32>,
    uv_scale: vec2<f32>,
    flags: u32,
    height_strength: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
    _pad4: u32,
    _pad5: u32,
};
@group(0) @binding(11) var<uniform> paint_params: PaintParams;

const PAINT_HAS_TEXTURE: u32 = 1u;
// Height channel (audit worklist #12): a mesh-pass texture binding
// feeding `height_perturbed_normal`, gated by this flag bit. Reuses
// `paint_sampler` and `paint_tile_uv` — same tile, same addressing.
const PAINT_HAS_HEIGHT_TEXTURE: u32 = 2u;
@group(0) @binding(12) var height_tex: texture_2d<f32>;
// ---- end paint tile ----

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    // World position for the position-map preview proxy (identity
    // transform in this pass — object space IS world space here).
    @location(1) world_pos: vec3<f32>,
    // Interpolated paint UV (the mesh's own uvs, uploaded verbatim —
    // see `renderer::Vertex`).
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_normal = in.normal;
    out.world_pos = in.position;
    out.uv = in.uv;
    return out;
}

const SUN_COLOR: vec3<f32> = vec3<f32>(1.0, 0.96, 0.90);
const SKY_COLOR: vec3<f32> = vec3<f32>(0.45, 0.55, 0.78);
const GROUND_COLOR: vec3<f32> = vec3<f32>(0.30, 0.27, 0.24);
const BASE_COLOR: vec3<f32> = vec3<f32>(0.72, 0.72, 0.75);

/// Tile-space UV: the bound texture holds ONE tile's texels, so a
/// global (possibly UDIM) uv is reduced to tile-local
/// `(uv - offset) * scale` before sampling. The single-tile default
/// (offset 0, scale 1) is the identity; UDIM tile (tu, tv) binds
/// offset (tu, tv), scale (1, 1) — the fractional part for uvs inside
/// the tile. Mirrors `renderer::PaintParams` byte-for-byte; see that
/// type's docs for the convention and the v1 single-texture boundary.
fn paint_tile_uv(uv: vec2<f32>) -> vec2<f32> {
    return (uv - paint_params.uv_offset) * paint_params.uv_scale;
}

/// The textured albedo: the bound tile at the interpolated uv when a
/// texture is set, else today's constant — so the unbound path renders
/// pixel-identical to the pre-texture shader. The flag branch is
/// uniform control flow, so the unsampled texture cannot leak into the
/// output (the `fallback_ignores_bound_map` family pins this).
fn paint_albedo(uv: vec2<f32>) -> vec3<f32> {
    if ((paint_params.flags & PAINT_HAS_TEXTURE) == 0u) {
        return BASE_COLOR;
    }
    return textureSample(paint_tex, paint_sampler, paint_tile_uv(uv)).rgb;
}

/// Height → normal, live, per frame (audit worklist #12): a
/// screen-space surface-gradient perturbation (Mikkelsen, "Bump
/// Mapping Unparametrized Surfaces on the GPU", 2010 — the same
/// formula behind Three.js's `perturbNormalArb`) built from `dpdx`/
/// `dpdy` of the INTERPOLATED world position instead of per-vertex
/// tangents, so a height stroke tilts the lit normal correctly
/// regardless of how the mesh's uvs sit relative to world space.
///
/// The height gradient itself comes from two explicit, unfiltered
/// `textureLoad`s (u+1, v+1 texel) rather than `dpdx`/`dpdy` of a
/// filtered sample — deterministic, no mip/filter-dependent
/// derivative, so the golden tests in `renderer.rs` can pin exact
/// bytes against it. Edge texels clamp (no neighbor past the last
/// row/column) rather than wrap.
///
/// # v1 boundary
///
/// `dpdx`/`dpdy` are per-primitive, screen-space derivatives: correct
/// within one triangle's flat facet, but with no notion of a curved
/// surface's true tangent frame across triangles — a heavily
/// triangulated or curved mesh gets a perturbation that's only exact
/// per-facet. True per-vertex tangents (uploaded alongside normals,
/// the bake pipeline's own `TRANSFER_BAKE_SHADER::transfer_tbn`
/// convention) are the named follow-up once the mesh pass carries
/// them — see this module's top doc and the kill report
/// (`docs/loop/kill-reports/2026-10-12-height-normal.md`).
fn height_perturbed_normal(n: vec3<f32>, world_pos: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    let tile_uv = paint_tile_uv(uv);
    let dims = textureDimensions(height_tex);
    let dims_f = vec2<f32>(dims);
    let max_texel = vec2<i32>(dims) - vec2<i32>(1, 1);
    let texel = clamp(vec2<i32>(floor(tile_uv * dims_f)), vec2<i32>(0, 0), max_texel);
    let texel_u1 = vec2<i32>(min(texel.x + 1, max_texel.x), texel.y);
    let texel_v1 = vec2<i32>(texel.x, min(texel.y + 1, max_texel.y));
    let h0 = textureLoad(height_tex, texel, 0).r;
    let hu = textureLoad(height_tex, texel_u1, 0).r;
    let hv = textureLoad(height_tex, texel_v1, 0).r;
    // UV-space height gradient, scaled by the per-tile strength.
    let dhdu = (hu - h0) * dims_f.x * paint_params.height_strength;
    let dhdv = (hv - h0) * dims_f.y * paint_params.height_strength;

    // Chain-rule into screen space via the uv's own screen-space
    // derivative, so the result doesn't assume uv axes line up with
    // world axes.
    let duv_dx = dpdx(tile_uv);
    let duv_dy = dpdy(tile_uv);
    let dhdx = dhdu * duv_dx.x + dhdv * duv_dx.y;
    let dhdy = dhdu * duv_dy.x + dhdv * duv_dy.y;

    // Mikkelsen's surface gradient: a local frame from the position's
    // own screen-space derivatives, no tangent attribute required.
    let dpos_dx = dpdx(world_pos);
    let dpos_dy = dpdy(world_pos);
    let r1 = cross(dpos_dy, n);
    let r2 = cross(n, dpos_dx);
    let det = dot(dpos_dx, r1);
    let surf_grad = sign(det) * (dhdx * r1 + dhdy * r2);
    return normalize(abs(det) * n - surf_grad);
}

/// Hemispherical environment irradiance: the bound convolved map when
/// `env_flags.flags == 1`, else the procedural stand-in — sky above the
/// horizon, ground below, blended by the normal's vertical component.
///
/// The procedural arm is byte-identical to the pre-IBL shader (the
/// fallback-regression tests pin this): the flag only *selects*, it
/// never reshapes the fallback math.
///
/// `equirect_uv` shares its formula with the convolve shader's
/// `ibl_equirect_uv` and the Rust `crate::ibl::equirect_uv_cpu` (see
/// that module's convention block): +Y is v=1 (last data row), -Y is
/// v=0. The `OPENPBR_SHADER` copy below is deliberately untouched —
/// its bind-group layout has no IBL set, and its env integration rides
/// the wave-5 specular tier, not this slice.
fn equirect_uv(n: vec3<f32>) -> vec2<f32> {
    let u = atan2(n.z, n.x) / 6.283185307179586 + 0.5;
    let v = asin(clamp(n.y, -1.0, 1.0)) / 3.14159265358979 + 0.5;
    return vec2<f32>(u, v);
}

fn environment_irradiance(n: vec3<f32>) -> vec3<f32> {
    if (env_flags.flags == 1u) {
        return textureSample(irradiance_tex, irradiance_sampler, equirect_uv(n)).rgb;
    }
    let t = clamp(n.y * 0.5 + 0.5, 0.0, 1.0);
    return mix(GROUND_COLOR, SKY_COLOR, t);
}

/// Split-Sum factor two without the LUT: the analytic fit of the
/// integrated GGX BRDF (Karis, "Real Shading in Unreal Engine 4",
/// SIGGRAPH 2013 — the `EnvBRDFApprox` closed form mobile pipelines
/// use in place of the 2D DFGLUT). Returns (scale, bias) with
/// specular = F0 * scale + bias. Mirrored op-for-op by
/// `crate::ibl::brdf_approx_cpu` (same literals, same order).
fn env_brdf_approx(n_dot_v: f32, roughness: f32) -> vec2<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = roughness * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * n_dot_v)) * r.x + r.y;
    return vec2<f32>(-1.04, 1.04) * a004 + r.zw;
}

/// Split-Sum factor one: cone-scaled prefiltered radiance at the
/// reflection vector, linearly blending the three fixed roughness
/// levels (0.0/0.5/1.0).
fn prefiltered_env(r: vec3<f32>, roughness: f32) -> vec3<f32> {
    let uv = equirect_uv(r);
    let p0 = textureSample(prefilter0_tex, irradiance_sampler, uv).rgb;
    let p1 = textureSample(prefilter1_tex, irradiance_sampler, uv).rgb;
    let p2 = textureSample(prefilter2_tex, irradiance_sampler, uv).rgb;
    let clo = clamp(roughness, 0.0, 1.0);
    if (clo < 0.5) {
        return mix(p0, p1, clo * 2.0);
    }
    return mix(p1, p2, (clo - 0.5) * 2.0);
}

/// Single-channel solo proxy (view modes — `crate::view_mode`):
/// one channel displayed raw, from already-bound data. The
/// per-channel GPU textures have no bindings in v1 (the paint
/// texture-set is document-model data; `SoloChannel::texture_binding`
/// reserves the future group-2 slots), so each arm reads the closest
/// bound source — except base color, which samples the bound paint
/// tile (core-loop kill 1): solo base-color IS the texture. The
/// Wave-2 texture handoff swaps the remaining arms to samples without
/// renumbering the ids.
fn solo_proxy(channel: u32, n: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    if (channel == VIEW_MODE_SOLO_BASE_COLOR) {
        // The bound paint tile at the interpolated uv — or the
        // uniform-material constant when nothing is bound.
        return paint_albedo(uv);
    }
    if (channel == VIEW_MODE_SOLO_EMISSIVE) {
        // No emission source in this pass.
        return vec3<f32>(0.0);
    }
    if (channel == VIEW_MODE_SOLO_ROUGHNESS) {
        let r = clamp(spec_params.roughness, 0.0, 1.0);
        return vec3<f32>(r);
    }
    if (channel == VIEW_MODE_SOLO_METALLIC) {
        let m = clamp(spec_params.metallic, 0.0, 1.0);
        return vec3<f32>(m);
    }
    if (channel == VIEW_MODE_SOLO_AO) {
        let e = environment_irradiance(n);
        return vec3<f32>(0.2126 * e.r + 0.7152 * e.g + 0.0722 * e.b);
    }
    if (channel == VIEW_MODE_SOLO_HEIGHT) {
        // No height source bound in v1 — mid-gray stand-in.
        return vec3<f32>(0.5);
    }
    // VIEW_MODE_SOLO_NORMAL: no tangent-space map bound yet — the
    // world normal remapped to 0..1 is the debug proxy.
    return n * 0.5 + vec3<f32>(0.5);
}

/// Mesh-map preview proxy (view modes): the bake output displayed
/// instead of the paint. Bake textures have no bindings in v1, so
/// each kind maps to the closest solo proxy; curvature, thickness,
/// and ID (no bound source at all) render mid-gray stand-ins until
/// the bake-texture handoff.
fn preview_proxy(kind: u32, n: vec3<f32>, world_pos: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    if (kind == VIEW_MODE_PREVIEW_AO) {
        return solo_proxy(VIEW_MODE_SOLO_AO, n, uv);
    }
    if (kind == VIEW_MODE_PREVIEW_NORMAL_BASE || kind == VIEW_MODE_PREVIEW_BENT_NORMAL
        || kind == VIEW_MODE_PREVIEW_WORLD_SPACE_NORMAL) {
        return solo_proxy(VIEW_MODE_SOLO_NORMAL, n, uv);
    }
    if (kind == VIEW_MODE_PREVIEW_POSITION) {
        // Unit-cell repeat of object space (this pass has no model
        // matrix — object space IS world space).
        return fract(world_pos);
    }
    if (kind == VIEW_MODE_PREVIEW_HEIGHT) {
        return solo_proxy(VIEW_MODE_SOLO_HEIGHT, n, uv);
    }
    return vec3<f32>(0.5);
}

/// Resolves the view-mode output: Lit keeps the PBR color, Unlit
/// drops the lighting math (albedo only — the texture when bound),
/// Solo/Preview substitute their proxy.
fn apply_view_mode(mode: u32, channel: u32, lit: vec3<f32>, n: vec3<f32>, world_pos: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    if (mode == VIEW_MODE_UNLIT) {
        return paint_albedo(uv);
    }
    if (mode == VIEW_MODE_SOLO) {
        return solo_proxy(channel, n, uv);
    }
    if (mode == VIEW_MODE_MESH_PREVIEW) {
        return preview_proxy(channel, n, world_pos, uv);
    }
    return lit;
}

/// The shaded, view-mode-resolved color: everything before the display
/// LUT. Shared by the two entry points (the LUT gate): `fs_main` (the
/// direct path, LUT applied here) and `fs_scene_linear` (the post-effects
/// path, scene-linear out; `crate::postfx_viewport`'s composite applies
/// the LUT after the chain).
fn shade(world_normal: vec3<f32>, world_pos: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    var n = normalize(world_normal);
    // Height → normal, live (audit worklist #12): uniform control
    // flow on a buffer-sourced flag, the same gate `paint_albedo`
    // uses for its texture — the unbound path (flag 0) never calls
    // `height_perturbed_normal`, so it's pixel-identical to today.
    if ((paint_params.flags & PAINT_HAS_HEIGHT_TEXTURE) != 0u) {
        n = height_perturbed_normal(n, world_pos, uv);
    }
    let to_light = normalize(-camera.light_dir.xyz);
    let diffuse = max(dot(n, to_light), 0.0);
    let env = environment_irradiance(n);
    // Environment acts as the ambient term; the sun adds directional
    // energy on top, weighted so lit surfaces approach full brightness.
    // The albedo is the bound paint tile (or the constant fallback).
    let albedo = paint_albedo(uv);
    var color = albedo * (env + diffuse * SUN_COLOR * 0.55);
    // Specular IBL tier (wave-5 v1): Split-Sum factor one x factor two,
    // gated on the map flag — the diffuse term above is byte-untouched.
    // v1 stand-ins: a uniform view (camera.eye is the view source, not
    // the world_pos varying — that varying serves the position-map
    // preview) and the SpecParams uniform
    // (roughness/metallic until the OpenPBR wiring exposes them).
    if (env_flags.flags == 1u) {
        let to_view = normalize(camera.eye.xyz);
        let reflect_dir = reflect(-to_view, n);
        let n_dot_v = clamp(dot(n, to_view), 0.0, 1.0);
        let rough = clamp(spec_params.roughness, 0.0, 1.0);
        let metal = clamp(spec_params.metallic, 0.0, 1.0);
        let f0 = mix(vec3<f32>(0.04), albedo, metal);
        let ab = env_brdf_approx(n_dot_v, rough);
        color = color + prefiltered_env(reflect_dir, rough) * (f0 * ab.x + ab.y);
    }
    // View modes (crate::view_mode): Lit keeps the PBR color
    // above; Unlit/Solo/Preview substitute their proxy.
    return apply_view_mode(view_mode.mode, view_mode.channel, color, n, world_pos, uv);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = shade(in.world_normal, in.world_pos, in.uv);
    // Data modes bypass the display LUT below (the §9 rule — the CPU
    // side binds the identity table for those modes, so the early
    // return agrees with the bound table; the branch additionally
    // protects a caller that passes the app LUT with a data mode).
    if (view_mode_bypasses_lut(view_mode.mode, view_mode.channel)) {
        return vec4<f32>(color, 1.0);
    }
    // Display LUT: the viewer chain, the last step before the target
    // (after diffuse + specular — the chain sees the finished color).
    // The identity table returns round(clamp(c)·255)/255, the byte the
    // unorm target would store for c anyway.
    return vec4<f32>(apply_display_lut(color), 1.0);
}

/// The post-effects path's entry (the LUT gate OFF): scene-linear color
/// into the Rgba16Float target, no clamp, no LUT. Alpha 1 marks mesh
/// coverage — the target clears to transparent, the chain carries
/// alpha through, and the composite blends on it.
@fragment
fn fs_scene_linear(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(shade(in.world_normal, in.world_pos, in.uv), 1.0);
}
"#;

/// [`MESH_SHADER`]'s direct-path fragment entry: the display LUT applied
/// inline (data view modes excepted), into the eframe surface.
pub const MESH_ENTRY_DIRECT: &str = "fs_main";

/// [`MESH_SHADER`]'s post-effects-path fragment entry: the LUT gated OFF,
/// scene-linear color into `crate::postfx_exec::POSTFX_FORMAT`.
pub const MESH_ENTRY_SCENE_LINEAR: &str = "fs_scene_linear";

/// Dab-splat compute pass: one workgroup per dab, rasterizing a filled,
/// radially-falloff'd circle into a storage texture with premultiplied
/// alpha-over compositing.
///
/// `Dab`'s field order/padding here must match `paint::Dab` byte-for-byte
/// (see that module's doc comment) — WGSL's `vec4<f32>` forces 16-byte
/// alignment, which Rust's `[f32; 4]` (4-byte aligned) does not provide for
/// free.
///
/// Workgroup-local contract (see `paint::PaintCompositor::splat_dabs`):
/// every invocation in a workgroup paints only pixels inside *its own*
/// dab's clamped bounding box, strided by `local_invocation_index` so no
/// two invocations in the same workgroup touch the same texel. There is no
/// such guarantee *across* workgroups in one dispatch — two dabs in the
/// same batch whose bounding boxes overlap would race on the shared
/// `read_write` storage texture (no atomics are used). The caller must
/// therefore keep spatially-overlapping dabs in separate `splat_dabs`
/// calls; wgpu's automatic hazard tracking orders successive compute
/// passes on the same texture correctly.
///
/// Quick-mask gate (§2's P1 row, `umber_brush::quick_mask`): a dab whose
/// `mask_gate` is set multiplies its per-texel coverage by `mask_tex` at
/// that texel (R8, `1.0` = paint allowed, `0.0` = blocked). The flag rides
/// per dab — not in a uniform — so gated and ungated dabs can share one
/// drain without a submit-time `write_buffer` reordering them. Ungated
/// dabs never load the mask (the bound texture may be a 1x1 fallback).
pub const PAINT_COMPUTE_SHADER: &str = r#"
struct Dab {
    pos: vec2<f32>,
    radius: f32,
    alpha: f32,
    color: vec4<f32>,
    hardness: f32,
    mask_gate: f32,
    falloff: u32,
    _pad: f32,
};

@group(0) @binding(0) var<storage, read> dabs: array<Dab>;
@group(0) @binding(1) var paint_tex: texture_storage_2d<rgba8unorm, read_write>;
@group(0) @binding(2) var<uniform> dims: vec2<u32>;
@group(0) @binding(3) var mask_tex: texture_2d<f32>;

const WORKGROUP_SIZE: u32 = 64u;

/// Radial falloff: 1.0 inside `hardness * radius`, then the dab's curve
/// down to the rim. `t` is the normalized distance from center (0 at
/// center, 1 at the edge); `e` is the remapped ring parameter (0 at the
/// core edge, 1 at the rim), `u = 1 - e`. Written without WGSL's built-in
/// `smoothstep` so the `hardness == 1.0` (hard disc) case can't divide by
/// zero. Ids are `paint::FALLOFF_*`; the CPU reference is
/// `umber_brush::falloff::FalloffCurve::eval` (same operations, same
/// order). Id 0 / unknown ids = the historical smoothstep.
fn radial_falloff(t: f32, hardness: f32, curve: u32) -> f32 {
    let denom = max(1.0 - hardness, 1e-4);
    let e = clamp((t - hardness) / denom, 0.0, 1.0);
    let u = 1.0 - e;
    switch curve {
        case 1u: { return u * u * u * (u * (u * 6.0 - 15.0) + 10.0); }
        case 2u: { return sqrt(max(2.0 * u - u * u, 0.0)); }
        case 3u: { return sqrt(u); }
        case 4u: { return u * u; }
        case 5u: { return u * u * u * u; }
        case 6u: { return u; }
        case 7u: { return u * (2.0 - u); }
        case 8u: { return 1.0; }
        case 0u, default: { return 1.0 - e * e * (3.0 - 2.0 * e); }
    }
}

@compute @workgroup_size(WORKGROUP_SIZE)
fn cs_main(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let dab = dabs[workgroup_id.x];
    let safe_radius = max(dab.radius, 1e-5);

    let min_xy = vec2<i32>(floor(dab.pos - safe_radius));
    let max_xy = vec2<i32>(ceil(dab.pos + safe_radius));
    let clamped_min = max(min_xy, vec2<i32>(0, 0));
    let clamped_max = min(max_xy, vec2<i32>(dims) - vec2<i32>(1, 1));
    if (clamped_max.x < clamped_min.x || clamped_max.y < clamped_min.y) {
        return;
    }

    let box_w = u32(clamped_max.x - clamped_min.x + 1);
    let box_h = u32(clamped_max.y - clamped_min.y + 1);
    let box_count = box_w * box_h;

    for (var i: u32 = local_index; i < box_count; i = i + WORKGROUP_SIZE) {
        let local_x = i32(i % box_w);
        let local_y = i32(i / box_w);
        let texel = clamped_min + vec2<i32>(local_x, local_y);

        let center = vec2<f32>(texel) + vec2<f32>(0.5, 0.5);
        let dist = distance(center, dab.pos);
        if (dist > safe_radius) {
            continue;
        }

        var coverage = dab.alpha * radial_falloff(dist / safe_radius, dab.hardness, dab.falloff);
        if (dab.mask_gate > 0.5) {
            coverage = coverage * textureLoad(mask_tex, texel, 0).r;
        }
        let src = dab.color * coverage;
        let dst = textureLoad(paint_tex, texel);
        let out = src + dst * (1.0 - src.a);
        textureStore(paint_tex, texel, out);
    }
}
"#;

/// OpenPBR Surface viewport shader — real-time subset (spec v1.1.1).
///
/// Implemented: base (weight/color/metalness), specular GGX (weight/
/// color/roughness/IOR), coat GGX lobe (weight/color/roughness/IOR)
/// with base roughening, emission, geometry opacity. Energy-conserving
/// lobe mixture per spec §3.10.
///
/// Deferred (documented deviations from the full spec): subsurface,
/// translucent base, thin-film, fuzz, anisotropy, thin-walled mode.
/// Schlick Fresnel replaces the exact dielectric Fresnel (spec §3.1
/// gives the exact form; the real-time deviation is standard and
/// visually indistinguishable in-viewport).
///
/// The `OpenPbrParams` uniform mirrors `material::OpenPbrParams`'s six
/// vec4 slots byte-for-byte (see that module's layout contract).
pub const OPENPBR_SHADER: &str = r#"
struct Camera {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    // World-space eye (CameraUniform::eye) — the view vector source.
    eye: vec4<f32>,
};

// Six vec4 slots — byte-identical to material::OpenPbrParams (96 bytes).
struct OpenPbrParams {
    base: vec4<f32>,     // xyz = base_color, w = base_weight
    surface: vec4<f32>,  // x = metalness, y = specular_roughness, z = coat_roughness, w = opacity
    specular: vec4<f32>, // xyz = specular_color, w = specular_weight
    coat: vec4<f32>,     // xyz = coat_color, w = coat_weight
    emission: vec4<f32>, // xyz = emission, w unused
    iors: vec4<f32>,     // x = specular_ior, y = coat_ior, zw unused
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> params: OpenPbrParams;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_normal = in.normal;
    out.world_pos = in.position;
    return out;
}

const SUN_COLOR: vec3<f32> = vec3<f32>(1.0, 0.96, 0.90);
const SKY_COLOR: vec3<f32> = vec3<f32>(0.45, 0.55, 0.78);
const GROUND_COLOR: vec3<f32> = vec3<f32>(0.30, 0.27, 0.24);

fn environment_irradiance(n: vec3<f32>) -> vec3<f32> {
    let t = clamp(n.y * 0.5 + 0.5, 0.0, 1.0);
    return mix(GROUND_COLOR, SKY_COLOR, t);
}

/// Schlick Fresnel (spec §3.1 deviation: exact dielectric form replaced
/// by Schlick — standard real-time approximation).
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - cos_theta, 5.0);
}

/// GGX/Trowbridge-Reitz normal distribution, alpha = roughness^2
/// (spec §3.1 microfacet model).
fn ggx_ndf(n_dot_h: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (3.14159265 * d * d + 1e-7);
}

/// Smith separable masking-shadowing, Schlick-GGX form (spec §3.1).
fn smith_g1(n_dot_v: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    return 2.0 * n_dot_v / (n_dot_v + sqrt(a2 + (1.0 - a2) * n_dot_v * n_dot_v) + 1e-7);
}

/// Energy-conserving coat: the coat lobe reflects first (Fresnel-weighted
/// at the coat IOR), and the base sees reduced, roughened energy
/// (spec §3.4: roughening + darkening-by-TIR is approximated by the
/// Fresnel split alone in this real-time subset).
fn coat_attenuation(n_dot_v: f32, coat_weight: f32, coat_ior: f32) -> f32 {
    // Approximate coat Fresnel F0 from IOR: ((ior-1)/(ior+1))^2.
    let f0 = (coat_ior - 1.0) / (coat_ior + 1.0);
    let f0sq = f0 * f0;
    let fc = f0sq + (1.0 - f0sq) * pow(1.0 - n_dot_v, 5.0);
    return (1.0 - coat_weight) + coat_weight * fc;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let to_light = normalize(-camera.light_dir.xyz);
    // True view vector from the camera eye (the uniform's trailing
    // field; see CameraUniform::with_eye).
    let to_view = normalize(camera.eye.xyz - in.world_pos);
    let n_dot_v = max(dot(n, to_view), 1e-4);
    let n_dot_l = max(dot(n, to_light), 0.0);
    let h = normalize(to_light + to_view);
    let n_dot_h = max(dot(n, h), 0.0);

    let metalness = clamp(params.surface.x, 0.0, 1.0);
    let spec_rough = clamp(params.surface.y, 0.0, 1.0);
    let coat_rough = clamp(params.surface.z, 0.0, 1.0);
    let opacity = clamp(params.surface.w, 0.0, 1.0);

    // Specular alpha (spec §3.1: alpha = roughness^2).
    let spec_alpha = spec_rough * spec_rough;
    let coat_alpha = coat_rough * coat_rough;

    // Specular F0: dielectric from IOR, tinted by specular_color;
    // metals use base_color as F0 (spec §3.2.1).
    let ior_f0 = (params.iors.x - 1.0) / (params.iors.x + 1.0);
    let dielectric_f0 = vec3<f32>(ior_f0 * ior_f0) * params.specular.xyz;
    let metal_f0 = params.base.xyz;
    let f0 = mix(dielectric_f0, metal_f0, metalness) * params.specular.w;

    // GGX specular lobe (Smith masking-shadowing).
    let spec_ndf = ggx_ndf(n_dot_h, spec_alpha);
    let spec_g = smith_g1(n_dot_v, spec_alpha) * smith_g1(n_dot_l, spec_alpha);
    let spec_f = fresnel_schlick(n_dot_h, f0);
    let specular_lobe = spec_ndf * spec_g * spec_f * n_dot_l;

    // Lambert diffuse (spec §3.2.2 gives Oren-Nayar; Lambert is the
    // accepted real-time simplification — documented deviation).
    let diffuse_albedo = params.base.xyz * params.base.w * (1.0 - metalness);
    let diffuse_lobe = diffuse_albedo * n_dot_l / 3.14159265;

    // Coat: second GGX lobe over the base, attenuating what reaches it.
    let coat_f0v = (params.iors.y - 1.0) / (params.iors.y + 1.0);
    let coat_f0sq = coat_f0v * coat_f0v;
    let coat_ndf = ggx_ndf(n_dot_h, coat_alpha);
    let coat_g = smith_g1(n_dot_v, coat_alpha) * smith_g1(n_dot_l, coat_alpha);
    let coat_f = fresnel_schlick(n_dot_h, vec3<f32>(coat_f0sq));
    let coat_lobe = coat_ndf * coat_g * coat_f * n_dot_l * params.coat.w;

    // Base energy after coat attenuation (approximate darkening).
    let base_atten = coat_attenuation(n_dot_v, params.coat.w, params.iors.y);

    // Emission is additive (spec §3.6).
    let emission = params.emission.xyz;

    // Direct-light mixture.
    let sun = SUN_COLOR;
    let direct = (diffuse_lobe + specular_lobe) * base_atten * sun + coat_lobe * sun;

    // Ambient: hemispherical env on the diffuse base (spec's full IBL
    // is deferred; the procedural hemisphere stands in as in MESH_SHADER).
    let env = environment_irradiance(n);
    let ambient = diffuse_albedo * env * base_atten;

    let color = direct + ambient + emission;
    return vec4<f32>(color, opacity);
}
"#;

/// Barycentric-edge wireframe overlay (Wave-4 item 7).
///
/// NOT a line list: the pass draws the same triangles (from the
/// duplicated-vertex buffer `umber_mesh::wire_vertices_from_indices`
/// builds — positions plus one unit-simplex corner each) and shades
/// edges in the fragment stage via the standard `fwidth` technique, so
/// lines stay ~1px at any zoom. Drawn AFTER the mesh in the same render
/// pass with `LessEqual` depth and a clip-space z bias (see the vertex
/// stage) so coplanar edges win the z-fight against the mesh surface.
///
/// Bindings: group 0 binding 0 is the SAME `Camera` uniform the mesh
/// pass uses — the pipeline reuses the mesh's bind-group layout object,
/// so the app hands the already-built camera bind group straight
/// through. Group 1 binding 0 is the wire color (user-set, default 40%
/// white — see `renderer::WireColor`).
pub const WIREFRAME_SHADER: &str = r#"
struct Camera {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    eye: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

struct WireColor {
    color: vec4<f32>,
};

@group(1) @binding(0) var<uniform> wire_color: WireColor;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) bary: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) bary: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    var clip = camera.view_proj * vec4<f32>(in.position, 1.0);
    // Z-fight mitigation (the design's clip-space bias arm): nudge the
    // edge toward the camera so it wins against the coplanar mesh
    // surface under LessEqual depth. 1e-3 of clip.w sits far above f32
    // depth noise (~1e-7 relative) yet far below any visible parallax.
    clip.z -= 0.001 * clip.w;
    out.clip_position = clip;
    out.bary = in.bary;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let edge = min(min(in.bary.x, in.bary.y), in.bary.z);
    let w = fwidth(edge);
    let alpha = 1.0 - smoothstep(0.0, w * 1.0, edge);
    if (alpha < 0.01) {
        discard;
    }
    return vec4<f32>(wire_color.color.rgb, wire_color.color.a * alpha);
}
"#;

/// Procedural infinite ground grid (Wave-4 item 7).
///
/// NOT a mesh: a fullscreen big-triangle pass whose fragment
/// reconstructs the world position per pixel by unprojecting the NDC
/// near/far points through the inverse view-proj and intersecting the
/// resulting ray with the y=0 plane. `world_from_ndc` below is the
/// shader-side copy of `crate::camera::world_from_ndc` (same formula —
/// the language boundary forces the duplication, documented here so
/// the next reader doesn't "dedupe" one side into drift); the Rust side
/// is shared by viewport picking and the offscreen-test math.
///
/// Pattern: minor lines every 1 world unit (25% white), major every 10
/// (thicker, 45% white), x-axis red-ish / z-axis blue-ish (the
/// Maya/Blender convention), infinite-grid fade to zero ~30 units from
/// the origin. Fragments below the alpha threshold discard so the clear
/// color shows through untouched.
///
/// Drawn BEFORE the mesh with depth-write OFF and depth-compare Always:
/// it is a reference plane, not geometry — the mesh overwrites it
/// wherever geometry exists.
pub const GRID_SHADER: &str = r#"
struct GridUniform {
    inv_view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> grid: GridUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // NDC passed through explicitly: `@builtin(position)` arrives in
    // the FRAGMENT stage in window (pixel) space, not clip space, so
    // the fragment cannot recover NDC from it — the vertex hands the
    // big-triangle XY down directly (w=1, so clip.xy IS ndc.xy).
    @location(0) ndc_xy: vec2<f32>,
};

/// Big-triangle fullscreen pass: no vertex buffer, positions from the
/// vertex index. z=0 (near plane) — the fragment ignores the depth and
/// re-derives world pos from NDC instead.
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.clip_position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.ndc_xy = positions[vertex_index];
    return out;
}

/// Shader-side copy of `crate::camera::world_from_ndc` — see that
/// function's doc comment for the sharing contract.
fn world_from_ndc(inv: mat4x4<f32>, ndc: vec4<f32>) -> vec3<f32> {
    let world = inv * ndc;
    return world.xyz / world.w;
}

/// Line coverage for a 1-unit grid along one axis: 0 at cell centers,
/// 1 on the line, ~1px wide via `fwidth` (screen-constant width).
fn grid_factor(coord: f32, pixel_world: f32) -> f32 {
    let dist = abs(fract(coord - 0.5) - 0.5);
    return 1.0 - smoothstep(0.0, pixel_world * 1.0, dist);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ndc_xy = in.ndc_xy;
    let ray_origin = world_from_ndc(grid.inv_view_proj, vec4<f32>(ndc_xy, 0.0, 1.0));
    let ray_far = world_from_ndc(grid.inv_view_proj, vec4<f32>(ndc_xy, 1.0, 1.0));
    let ray_dir = ray_far - ray_origin;
    // Looking parallel to (or away from) the plane: no grid here.
    if (abs(ray_dir.y) < 1e-6) {
        discard;
    }
    let t = -ray_origin.y / ray_dir.y;
    if (t < 0.0) {
        discard;
    }
    let world = ray_origin + ray_dir * t;

    let pixel_world = length(vec2<f32>(fwidth(world.x), fwidth(world.z)));
    let minor = max(grid_factor(world.x, pixel_world), grid_factor(world.z, pixel_world));
    // Major lines every 10 units: distance rescaled to world units so
    // the same pixel width reads thicker and brighter.
    let major_dist = abs(fract(world.x / 10.0 - 0.5) - 0.5) * 10.0;
    let major_dist_z = abs(fract(world.z / 10.0 - 0.5) - 0.5) * 10.0;
    let major = max(
        1.0 - smoothstep(0.0, pixel_world * 1.5, major_dist),
        1.0 - smoothstep(0.0, pixel_world * 1.5, major_dist_z)
    );
    // Axis highlight: the z=0 line runs along x (red-ish), the x=0
    // line along z (blue-ish).
    let axis_x = 1.0 - smoothstep(0.0, pixel_world * 1.5, abs(world.z));
    let axis_z = 1.0 - smoothstep(0.0, pixel_world * 1.5, abs(world.x));

    // Infinite-grid fade: full strength at the origin, gone by 30u.
    let fade = 1.0 - smoothstep(0.0, 30.0, length(world.xz));

    var color = vec3<f32>(0.25) * minor + vec3<f32>(0.45) * major;
    color = mix(color, vec3<f32>(0.80, 0.25, 0.25), axis_x);
    color = mix(color, vec3<f32>(0.25, 0.40, 0.90), axis_z);
    let alpha = max(max(minor, major), max(axis_x, axis_z)) * fade;
    if (alpha < 0.01) {
        discard;
    }
    return vec4<f32>(color, alpha);
}
"#;
