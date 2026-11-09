//! WGSL shader sources for the bake engine's compute-raycast pass.
//!
//! Wave 3 scope (docs/specs/requirements.md §3: "Compute-shader raycast/bvh
//! bake path (no RT hardware dependency)"). This first slice is the
//! raycast *core*: brute-force Möller–Trumbore against every triangle, no
//! acceleration structure — see [`AO_BAKE_SHADER`]'s own doc comment for
//! the perf budget and what a BVH slice would change.

/// Ambient-occlusion compute-raycast pass: one workgroup per output texel,
/// raycasting a hemisphere of rays from a parameter plane against a
/// brute-force triangle list.
///
/// # Why a parameter plane, not a UV-mapped position map
///
/// The eventual bake target is "one texel per UV location on the mesh
/// being baked," which needs a position map (UV → world position,
/// produced by rasterizing the mesh's own UV layout) as this pass's input.
/// Building that rasterizer is deferred to the next slice — see
/// `umber-bake/LANDING_NOTES_AO.md`. This slice instead raycasts from a
/// user-described plane (`PlaneUniform`: world origin + two axes + an
/// extent), so every texel's world position is a closed-form function of
/// its (u, v) grid coordinate instead of a mesh lookup. That proves the
/// raycast core — hemisphere sampling, ray-triangle intersection,
/// occlusion accumulation — independently of the position-map work, with
/// the two slices composing cleanly later (the position map will replace
/// only the "texel → world position" step below; everything from the
/// hemisphere sample onward is unchanged).
///
/// # Layout contracts
///
/// - `Tri` must match `umber_bake::ao`'s private `GpuTriangle` byte-for-byte
///   (four `vec4<f32>`s: three vertex positions, then the face normal, all
///   `w` components unused padding — the WGSL `vec4` 16-byte alignment is
///   free here since every field is already 16 bytes, unlike `Dab`'s mixed
///   layout in `shaders::PAINT_COMPUTE_SHADER`).
/// - `PlaneUniform`/`AoParams` must match `umber_bake::ao`'s private
///   `PlaneDesc`/`AoUniform` byte-for-byte; see those types' doc comments
///   for the explicit padding fields that make a `[f32; 3]` (Rust, 4-byte
///   aligned) line up with a WGSL `vec3<f32>` (16-byte aligned).
///
/// # Perf budget (this slice)
///
/// No BVH: every ray tests every triangle in the storage buffer, and every
/// texel's workgroup re-walks the same list. Cost is
/// `texels * rays * triangle_count`. At `rays = 16` this slice targets
/// meshes under ~5k triangles on a `512²` target (~4.1M ray-triangle tests
/// per full bake) — acceptable for a correctness-proving first slice, not
/// for production asset sizes. A BVH (or at minimum a uniform grid) over
/// the triangle list is the next slice's performance work; the shader's
/// outer-loop shape (`for t in 0..tri_count`) is exactly where that
/// acceleration structure would slot in without touching the hemisphere
/// sampling or occlusion-accumulation code around it.
///
/// # Sampling + reduction
///
/// Hemisphere directions are deterministic and stratified (uniform in
/// `cos(theta)`, golden-angle in `phi`) rather than randomized — see
/// `hemisphere_sample`'s doc comment for why a per-texel RNG was skipped.
/// Each workgroup (`@workgroup_size(64)`, matching
/// `shaders::PAINT_COMPUTE_SHADER`'s convention) owns exactly one texel;
/// invocations split the ray budget by `local_invocation_index` and
/// accumulate hits into a `var<workgroup> atomic<u32>` counter guarded by
/// `workgroupBarrier()`s, so the result doesn't depend on `rays` dividing
/// evenly into the workgroup size (a `rays < 64` config, e.g. the default
/// 16, simply leaves some invocations idle, not racing).
///
/// # Storage-texture access: `write`, not `read_write`
///
/// Unlike `shaders::PAINT_COMPUTE_SHADER` (which blends into existing
/// texel contents and therefore needs `read_write`, gated behind
/// `wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` on
/// `Rgba8Unorm` — see `paint::PaintError::MissingDeviceFeature`), this
/// pass writes each texel exactly once with no dependency on its prior
/// value. `write`-only storage-texture access on `Rgba8Unorm` is a core
/// WebGPU capability (unlike `read`/`read_write`, which are the
/// non-portable adapter extension for any format besides the `r32`
/// family), so this pass needs no extra device feature — confirmed by the
/// GPU test in `umber-bake::ao` requesting a plain default device.
pub const AO_BAKE_SHADER: &str = r#"
struct Tri {
    v0: vec4<f32>,
    v1: vec4<f32>,
    v2: vec4<f32>,
    normal: vec4<f32>,
};

struct PlaneUniform {
    origin: vec3<f32>,
    _pad0: f32,
    u_axis: vec3<f32>,
    _pad1: f32,
    v_axis: vec3<f32>,
    _pad2: f32,
    extent: vec2<f32>,
    _pad3: vec2<f32>,
};

struct AoParams {
    plane: PlaneUniform,
    rays: u32,
    max_distance: f32,
    bias: f32,
    tri_count: u32,
    // Bent-normal toggle (wave-4 item 5 slice 2): 0 = AO only, 1 = the
    // mesh-fed entry also accumulates unoccluded ray directions into
    // `bent_tex` (see `cs_main_from_position`'s doc comment for the frame
    // and encoding contract). Additive at the END so the pre-existing
    // prefix layout stays stable; the three trailing pads round the struct
    // to 96 bytes (WGSL uniform structs are 16-byte aligned), matching
    // `umber_bake::ao::AoUniform` — see that type's doc comment. `cs_main`
    // (the parameter-plane path) ignores these words entirely.
    bent_normals: u32,
    _pad4: u32,
    _pad5: u32,
    _pad6: u32,
};

@group(0) @binding(0) var<storage, read> triangles: array<Tri>;
@group(0) @binding(1) var ao_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: AoParams;
@group(0) @binding(3) var<uniform> dims: vec2<u32>;
@group(0) @binding(6) var bent_tex: texture_storage_2d<rgba8unorm, write>;

var<workgroup> hit_count: atomic<u32>;
// Bent-normal second accumulator (one partial sum per invocation —
// `atomic<u32>` works for AO's hit *count* but a direction *sum* is a
// `vec3<f32>` with no atomic add in core WGSL, so each invocation keeps its
// own partial sum here and invocation 0 reduces them serially). 64 entries
// of 16-byte-strided `vec3` is 1 KiB, well under the measured 16 KiB
// workgroup-storage budget (see POSITION_BAKE_SHADER's doc comment).
var<workgroup> bent_parts: array<vec3<f32>, WORKGROUP_SIZE>;

const WORKGROUP_SIZE: u32 = 64u;
const PI: f32 = 3.14159265358979;
const GOLDEN_CONJ: f32 = 0.6180339887498949;
const EPS: f32 = 1e-6;

/// Deterministic hemisphere direction for ray `i` of `n`, in a local frame
/// where +Z is the hemisphere pole: `cos(theta)` is stratified evenly
/// across `[0, 1)` (uniform over solid angle) and `phi` walks the golden
/// angle, giving low-discrepancy coverage without a per-texel random seed.
/// A fixed sample set (rather than per-texel jitter) keeps this slice's
/// output deterministic texel-to-texel, which is what the GPU test's
/// hand-derived occlusion bounds rely on.
fn hemisphere_sample(i: u32, n: u32) -> vec3<f32> {
    let nf = max(f32(n), 1.0);
    let cos_theta = 1.0 - (f32(i) + 0.5) / nf;
    let sin_theta = sqrt(max(0.0, 1.0 - cos_theta * cos_theta));
    let phi = 2.0 * PI * fract(f32(i) * GOLDEN_CONJ);
    return vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), cos_theta);
}

/// Möller–Trumbore ray-triangle intersection. Double-sided (no backface
/// culling — requirements §3's "ignore backfaces" bake setting is deferred
/// to a later slice). Returns true iff the hit parameter `t` lands in the
/// open interval `(EPS, max_t)`.
fn hits_triangle(
    orig: vec3<f32>,
    dir: vec3<f32>,
    v0: vec3<f32>,
    v1: vec3<f32>,
    v2: vec3<f32>,
    max_t: f32,
) -> bool {
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let h = cross(dir, e2);
    let a = dot(e1, h);
    if (abs(a) < EPS) {
        return false;
    }
    let f = 1.0 / a;
    let s = orig - v0;
    let u = f * dot(s, h);
    if (u < 0.0 || u > 1.0) {
        return false;
    }
    let q = cross(s, e1);
    let v = f * dot(dir, q);
    if (v < 0.0 || u + v > 1.0) {
        return false;
    }
    let t = f * dot(e2, q);
    return t > EPS && t < max_t;
}

@compute @workgroup_size(WORKGROUP_SIZE)
fn cs_main(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    if (local_index == 0u) {
        atomicStore(&hit_count, 0u);
    }
    workgroupBarrier();

    let dims_f = vec2<f32>(dims);
    let texel_center = (vec2<f32>(workgroup_id.xy) + vec2<f32>(0.5, 0.5)) / dims_f;
    let local_uv = (texel_center - vec2<f32>(0.5, 0.5)) * params.plane.extent;

    let u_axis = params.plane.u_axis;
    let v_axis = params.plane.v_axis;
    let normal = normalize(cross(u_axis, v_axis));
    let surface_pos = params.plane.origin + local_uv.x * u_axis + local_uv.y * v_axis;
    let ray_origin = surface_pos + params.bias * normal;

    let n = params.rays;
    for (var i: u32 = local_index; i < n; i = i + WORKGROUP_SIZE) {
        let local_dir = hemisphere_sample(i, n);
        let dir = normalize(local_dir.x * u_axis + local_dir.y * v_axis + local_dir.z * normal);

        var hit = false;
        for (var t: u32 = 0u; t < params.tri_count; t = t + 1u) {
            let tri = triangles[t];
            if (hits_triangle(ray_origin, dir, tri.v0.xyz, tri.v1.xyz, tri.v2.xyz, params.max_distance)) {
                hit = true;
                break;
            }
        }
        if (hit) {
            atomicAdd(&hit_count, 1u);
        }
    }
    workgroupBarrier();

    if (local_index == 0u) {
        let blocked = f32(atomicLoad(&hit_count)) / f32(max(n, 1u));
        let ao = clamp(1.0 - blocked, 0.0, 1.0);
        textureStore(ao_tex, vec2<i32>(workgroup_id.xy), vec4<f32>(ao, ao, ao, 1.0));
    }
}

// --- Mesh-fed variant: reads the position-map pass's output instead of a
// parameter plane. See this file's module doc comment ("Mesh-fed AO entry
// point") for why this lives as a second entry point in the same module
// rather than a separate shader constant: it reuses `triangles`, `ao_tex`,
// `params`, `hit_count`, `hemisphere_sample`, and `hits_triangle` verbatim.

@group(0) @binding(4) var position_tex: texture_2d<f32>;
@group(0) @binding(5) var normal_tex: texture_2d<f32>;

struct Basis {
    b1: vec3<f32>,
    b2: vec3<f32>,
};

/// Branchless tangent-frame construction from a unit normal (Duff et al.,
/// "Building an Orthonormal Basis, Revisited", JCGT 2017). `cs_main`'s
/// plane hands the shader `u_axis`/`v_axis` directly; `cs_main_from_position`
/// only has a per-texel normal (the position pass's second output) and
/// needs *some* consistent tangent frame to steer `hemisphere_sample`'s
/// local directions into world space. Any orthonormal frame works — AO is
/// isotropic in `phi`, so the choice doesn't bias the result — this one was
/// picked for being branchless and numerically stable at the south-pole
/// case other common formulas (e.g. a naive `cross(n, vec3(0,0,1))`) fail.
fn orthonormal_basis(n: vec3<f32>) -> Basis {
    let sign_z = select(-1.0, 1.0, n.z >= 0.0);
    let a = -1.0 / (sign_z + n.z);
    let b = n.x * n.y * a;
    return Basis(
        vec3<f32>(1.0 + sign_z * n.x * n.x * a, sign_z * b, -sign_z * n.x),
        vec3<f32>(b, sign_z + n.y * n.y * a, -n.y),
    );
}

/// Mesh-fed ambient occlusion (+ optional bent normals): identical
/// hemisphere-raycast core to `cs_main`, but the per-texel ray origin and
/// tangent frame come from `position_tex`/`normal_tex` (the two outputs of
/// `POSITION_BAKE_SHADER`'s `cs_main`, see `bake_shaders::POSITION_BAKE_SHADER`)
/// rather than a closed-form plane. One workgroup per texel, matching `cs_main`'s
/// dispatch convention (`dispatch_workgroups(width, height, 1)`), so
/// `workgroup_id.xy` is directly the texel coordinate into both input
/// textures and `ao_tex`.
///
/// `position_tex`'s alpha channel is the position pass's coverage flag (see
/// that shader's doc comment): `0.0` means no UV triangle covered this
/// texel, so there is no surface to raycast from. Every invocation in a
/// workgroup loads the same texel's `covered` flag (it's a per-workgroup,
/// not per-invocation, value here, unlike `POSITION_BAKE_SHADER`'s 8x8-tile
/// layout where each invocation owns a *different* texel) — so branching on
/// it is workgroup-uniform and safe around the `workgroupBarrier()`s below.
/// Uncovered texels skip the raycast entirely and write `(0, 0, 0, 0)`,
/// distinguishable on readback from a fully-occluded-but-covered texel
/// (`(0, 0, 0, 1)`) by alpha alone — which is why `ao::bake_ao_mesh` returns
/// full RGBA8 instead of `ao::run`'s R-channel-only bytes.
///
/// # Bent normals (`params.bent_normals != 0`, wave-4 item 5 slice 2)
///
/// When enabled, the visibility loop doubles as a bent-normal accumulator
/// (Substance's shared-sample approach: the bent map reuses AO's rays, no
/// second dispatch): every ray that produces NO hit adds its world-space
/// direction to the invoking invocation's `bent_parts` slot, and invocation
/// 0 reduces the slots into the texel's bent vector.
///
/// Frame: the accumulated directions are the SAME world-space vectors the
/// hit test consumes (`local_dir` steered by the Duff-et-al.
/// `orthonormal_basis` frame — `dir = lx*b1 + ly*b2 + lz*normal`), so the
/// sum lives in world space, not tangent space. Output encoding follows the
/// other normal bakers' `* 0.5 + 0.5` convention (see
/// `umber_bake::normal_map`'s `tangent_normal * 0.5 + 0.5`, here applied to
/// the world-space bent vector): `rgb = bent * 0.5 + 0.5`,
/// `a = |sum| / ray_count` as confidence. A zero sum (fully occluded)
/// writes the geometric normal (`normal_tex`) encoded the same way with
/// `a = 0`; an uncovered texel writes `(0, 0, 0, 0)`, matching AO's
/// background convention. When the toggle is 0 the bent stores are skipped
/// and `ao_tex` output is bit-for-bit what the pre-bent shader wrote (the
/// AO path is untouched — integer hit counting, no shared state with the
/// accumulator).
@compute @workgroup_size(WORKGROUP_SIZE)
fn cs_main_from_position(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let coord = vec2<i32>(workgroup_id.xy);
    let sample = textureLoad(position_tex, coord, 0);
    let covered = sample.w > 0.5;

    if (local_index == 0u) {
        atomicStore(&hit_count, 0u);
    }
    workgroupBarrier();

    if (covered) {
        let surface_pos = sample.xyz;
        let normal = normalize(textureLoad(normal_tex, coord, 0).xyz);
        let basis = orthonormal_basis(normal);
        let ray_origin = surface_pos + params.bias * normal;

        let n = params.rays;
        // Per-invocation bent partial sum (world-space unoccluded
        // directions; reduced by invocation 0 below). Kept at zero when the
        // toggle is off so the gated add never executes.
        var bent_part = vec3<f32>(0.0, 0.0, 0.0);
        for (var i: u32 = local_index; i < n; i = i + WORKGROUP_SIZE) {
            let local_dir = hemisphere_sample(i, n);
            let dir = normalize(
                local_dir.x * basis.b1 + local_dir.y * basis.b2 + local_dir.z * normal
            );

            var hit = false;
            for (var t: u32 = 0u; t < params.tri_count; t = t + 1u) {
                let tri = triangles[t];
                if (hits_triangle(ray_origin, dir, tri.v0.xyz, tri.v1.xyz, tri.v2.xyz, params.max_distance)) {
                    hit = true;
                    break;
                }
            }
            if (hit) {
                atomicAdd(&hit_count, 1u);
            } else if (params.bent_normals != 0u) {
                bent_part = bent_part + dir;
            }
        }
        bent_parts[local_index] = bent_part;
    } else {
        bent_parts[local_index] = vec3<f32>(0.0, 0.0, 0.0);
    }
    workgroupBarrier();

    if (local_index == 0u) {
        if (covered) {
            let blocked = f32(atomicLoad(&hit_count)) / f32(max(params.rays, 1u));
            let ao = clamp(1.0 - blocked, 0.0, 1.0);
            textureStore(ao_tex, coord, vec4<f32>(ao, ao, ao, 1.0));
            if (params.bent_normals != 0u) {
                // Serial reduction over the invocation slots (fixed order,
                // so the bent sum is deterministic texel-to-texel like the
                // AO hit count; for `rays <= WORKGROUP_SIZE` each slot holds
                // exactly one ray's direction, matching a serial 0..n sum).
                var bent_sum = vec3<f32>(0.0, 0.0, 0.0);
                for (var k: u32 = 0u; k < WORKGROUP_SIZE; k = k + 1u) {
                    bent_sum = bent_sum + bent_parts[k];
                }
                let sum_len = length(bent_sum);
                if (sum_len > 0.0) {
                    let bent = bent_sum / sum_len;
                    let confidence = clamp(sum_len / f32(max(params.rays, 1u)), 0.0, 1.0);
                    textureStore(bent_tex, coord, vec4<f32>(bent * 0.5 + 0.5, confidence));
                } else {
                    // Fully occluded: no unoccluded direction exists, so
                    // fall back to the geometric normal with zero
                    // confidence (see this entry point's doc comment).
                    // (`normal` from the raycast block above is out of
                    // scope here — reload the same texel.)
                    let geom = normalize(textureLoad(normal_tex, coord, 0).xyz);
                    textureStore(bent_tex, coord, vec4<f32>(geom * 0.5 + 0.5, 0.0));
                }
            }
        } else {
            textureStore(ao_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
            if (params.bent_normals != 0u) {
                textureStore(bent_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
            }
        }
    }
}
"#;

/// UV→world-position map: per texel, the world position and face normal
/// of the covering triangle, the input every mesh-map baker (AO,
/// curvature, thickness, normal) consumes. See `umber_bake::position` for
/// the driver side.
///
/// # Row 156-B kill 5b: a gather over the UV G-buffer
///
/// Coverage, the winning triangle and its barycentrics are READ from the
/// UV G-buffer ([`UV_GBUFFER_SHADER`]'s `prim_tex` + `bary_tex`), the GPU
/// mirror of `umber_mesh::UvGBuffer`. This shader used to carry its own
/// chunked point-in-triangle solve (the last private copy); it now only
/// does the per-covered-texel work: `pos = v0*b0 + v1*b1 + v2*b2` and the
/// triangle's face normal.
///
/// # Layout contracts
///
/// - `PosTri` matches `umber_bake::position::GpuPosTri` (80 bytes: three
///   world positions with UV x in `w`, UV y in `uv_y`, the face normal).
///   UVs are unused here; the layout stays shared with the transfer and
///   AO triangle builders.
/// - Outputs `pos_tex` / `normal_tex`: `Rgba32Float`, `w` = coverage
///   (1.0 covered, 0.0 with xyz = 0 uncovered), the alpha convention every
///   downstream pass gates on (`<= 0.5` = uncovered).
/// - `@workgroup_size(8, 8)`, dispatched `ceil(w/8) x ceil(h/8)`.
pub const POSITION_BAKE_SHADER: &str = r#"
struct PosTri {
    v0: vec4<f32>,
    v1: vec4<f32>,
    v2: vec4<f32>,
    uv_y: vec4<f32>,
    normal: vec4<f32>,
};

@group(0) @binding(0) var<storage, read> tris: array<PosTri>;
@group(0) @binding(1) var pos_tex: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var normal_tex: texture_storage_2d<rgba32float, write>;
@group(0) @binding(3) var prim_tex: texture_2d<u32>;
@group(0) @binding(4) var bary_tex: texture_2d<f32>;

const NO_PRIM: u32 = 0xFFFFFFFFu;

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(pos_tex);
    if (gid.x >= dims.x || gid.y >= dims.y) {
        return;
    }
    let coord = vec2<i32>(i32(gid.x), i32(gid.y));
    let prim = textureLoad(prim_tex, coord, 0).x;
    if (prim == NO_PRIM) {
        textureStore(pos_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        textureStore(normal_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }
    let b = textureLoad(bary_tex, coord, 0).xyz;
    let tri = tris[prim];
    let pos = tri.v0.xyz * b.x + tri.v1.xyz * b.y + tri.v2.xyz * b.z;
    textureStore(pos_tex, coord, vec4<f32>(pos, 1.0));
    textureStore(normal_tex, coord, vec4<f32>(normalize(tri.normal.xyz), 1.0));
}
"#;

/// Screen-space curvature estimation from a position/normal map: one
/// workgroup per output texel, reading the 4-neighborhood's normals and
/// positions to estimate signed directional curvature.
///
/// # Estimator
///
/// Per covered texel, over the in-bounds, covered 4-neighbors (`+x`, `-x`,
/// `+y`, `-y`):
///
/// ```text
/// k_i = dot(n_i - n_0, normalize(p_i - p_0)) / max(length(p_i - p_0), 1e-6)
/// curv = clamp(-strength * mean(k_i), -1, 1)
/// ```
///
/// This is the discrete directional-curvature estimator used by screen-space
/// bakers (e.g. Blender's pointiness-adjacent cavity approximations and
/// various "curvature from normal buffer" post passes): where the surface
/// bends, neighboring normals differ along the direction of travel, and
/// dividing by the travel distance turns that difference into a
/// curvature-scale quantity. Averaging the four axis directions makes it
/// rotation-tolerant without a full multi-ring fit (see
/// `umber-bake/LANDING_NOTES_CURVATURE.md` for sources and limits).
///
/// # Sign convention (MeshLab: convex = negative = darker)
///
/// The raw `dot(dn, dir) / len` term is *positive* on an outward bulge
/// (sphere check: `n(p) = p/R`, so `n_i - n_0 = (p_i - p_0)/R` points along
/// the travel direction and the dot product is `|dp|/R > 0`). The leading
/// minus flips it so convex (outward-bulge) regions read *negative* and
/// bake *darker*, concave crevices positive/brighter — MeshLab's
/// convention, where mean curvature is negative on convex parts. Documented
/// here (not just in the landing notes) because the minus is otherwise an
/// inviting "simplification" for a future reader to delete.
///
/// # Layout contracts
///
/// - `CurvatureParams` must match `umber_bake::curvature`'s private
///   `CurvatureUniform` byte-for-byte (`width`, `height`, `strength`, one
///   `f32` pad — 16 bytes, already a multiple of WGSL's 16-byte
///   uniform-struct alignment, so no further padding is needed).
/// - `position_tex`/`normal_tex` are the two `Rgba32Float` outputs of
///   `POSITION_BAKE_SHADER`'s `cs_main`, bound read-only and sampled with
///   `textureLoad` at integer texel coordinates (no sampler, no filtering
///   — filtering would smear normals across UV seams; see the landing
///   notes). `position_tex`'s alpha is the position pass's coverage flag:
///   `0.0` means no UV triangle covered that texel.
/// - `out_tex` is a write-only `Rgba8Unorm` storage texture (core WebGPU,
///   no device feature — the same reason `AO_BAKE_SHADER` uses `write`,
///   not `read_write`): `rgb` is `(curv + 1) / 2` grayscale, `a` is
///   coverage (`1.0` covered, `0.0` uncovered).
///
/// # Dispatch + edge behavior
///
/// One workgroup (`@workgroup_size(1)`) per texel, dispatched as
/// `dispatch_workgroups(width, height, 1)`, so `workgroup_id.xy` is
/// directly the texel coordinate into all three textures — the same
/// "one workgroup per texel" shape as `AO_BAKE_SHADER`'s
/// `cs_main_from_position`, minus the ray loop (each texel's work here is
/// four neighbor loads, so the 64-invocation workgroup would sit idle).
/// Neighbor coordinates are clamped explicitly against `params.width` /
/// `params.height` (never sampled out of bounds); out-of-bounds and
/// uncovered neighbors are *skipped, not zero-filled* (a hole must not
/// flatten a neighboring ridge), and a covered texel with no valid
/// neighbors at all (isolated single-texel island) bakes mid-gray
/// (`curv = 0`). Uncovered texels write `(0, 0, 0, 0)`, distinguishable
/// from a flat-but-covered texel (`(~0.5, ~0.5, ~0.5, 1)`) by alpha alone
/// — the same alpha convention `cs_main_from_position` uses, which is why
/// `curvature::bake_curvature_mesh` returns full RGBA8.
pub const CURVATURE_BAKE_SHADER: &str = r#"
struct CurvatureParams {
    width: u32,
    height: u32,
    strength: f32,
    _pad: f32,
};

@group(0) @binding(0) var out_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(1) var<uniform> params: CurvatureParams;
@group(0) @binding(2) var position_tex: texture_2d<f32>;
@group(0) @binding(3) var normal_tex: texture_2d<f32>;

const CURV_EPS: f32 = 1e-6;

@compute @workgroup_size(1)
fn cs_main(@builtin(workgroup_id) workgroup_id: vec3<u32>) {
    let coord = vec2<i32>(workgroup_id.xy);
    let center = textureLoad(position_tex, coord, 0);
    if (center.w <= 0.5) {
        textureStore(out_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    let p0 = center.xyz;
    let n0 = normalize(textureLoad(normal_tex, coord, 0).xyz);
    let dims = vec2<i32>(i32(params.width), i32(params.height));

    var sum = 0.0;
    var count = 0u;
    for (var k = 0u; k < 4u; k = k + 1u) {
        var off = vec2<i32>(1, 0);
        if (k == 1u) {
            off = vec2<i32>(-1, 0);
        } else if (k == 2u) {
            off = vec2<i32>(0, 1);
        } else if (k == 3u) {
            off = vec2<i32>(0, -1);
        }
        let nc = coord + off;
        if (nc.x < 0 || nc.y < 0 || nc.x >= dims.x || nc.y >= dims.y) {
            continue;
        }
        let s = textureLoad(position_tex, nc, 0);
        if (s.w <= 0.5) {
            continue;
        }
        let dp = s.xyz - p0;
        let len = length(dp);
        let dir = dp / max(len, CURV_EPS);
        let dn = textureLoad(normal_tex, nc, 0).xyz - n0;
        sum = sum + dot(dn, dir) / max(len, CURV_EPS);
        count = count + 1u;
    }

    var curv = 0.0;
    if (count > 0u) {
        // Negated: the raw estimator is positive on outward bulges (see
        // this constant's doc comment); MeshLab convention wants convex
        // negative (= darker).
        curv = -sum / f32(count) * params.strength;
    }
    curv = clamp(curv, -1.0, 1.0);
    let g = (curv + 1.0) * 0.5;
    textureStore(out_tex, coord, vec4<f32>(g, g, g, 1.0));
}
"#;

/// Thickness (local solid depth) estimation by inward hemisphere raycasting:
/// one workgroup per output texel, casting rays *into* the mesh and recording
/// the nearest opposite-face hit.
///
/// # Estimator
///
/// Per covered texel, over `params.rays` deterministic stratified hemisphere
/// directions (the same `hemisphere_sample` as [`AO_BAKE_SHADER`], steered by
/// the same Duff-et-al. `orthonormal_basis` tangent frame from the position
/// pass's per-texel normal), pointed into the surface — i.e. around the
/// *negative* normal instead of [`AO_BAKE_SHADER`]'s outward `+normal`
/// hemisphere:
///
/// ```text
/// min_dist = min over rays, over triangles of hit t in (EPS, max_distance)
/// thickness = clamp(min_dist / max_distance, 0, 1)
/// ```
///
/// A per-ray miss contributes nothing (that ray simply never lowers
/// `min_dist`, which starts at `max_distance`); a texel whose *every* ray
/// misses therefore bakes `1.0`. That is a deliberate convention, not a
/// fallback: a miss means the ray escaped through open geometry, so the
/// surface reads as "no opposite face within range" — i.e. maximally thick
/// (see `umber-bake/LANDING_NOTES_THICKNESS.md` for the open-mesh artifact
/// this implies). Output is grayscale `rgb = thickness`, `a = coverage
/// (`1.0` covered, `0.0` uncovered).
///
/// Uncovered texels (the position pass's coverage flag `<= 0.5`) skip the
/// raycast entirely and write `(0, 0, 0, 0)`, distinguishable on readback
/// from a covered-but-maximally-thick texel (`(1, 1, 1, 1)`) by alpha alone
/// — the same alpha convention `AO_BAKE_SHADER::cs_main_from_position` uses.
///
/// # Why into the mesh, and where the ray starts
///
/// `ray_origin = surface_pos - bias * normal` (nudged *inside* along the
/// negative normal, the mirror of AO's outward `+ bias * normal`). Starting
/// outside and casting inward would cross the originating surface itself at
/// `t ~= bias` and bake a false near-zero thickness; starting just inside
/// puts the origin triangle behind every inward ray so only genuine
/// opposite faces lower `min_dist`. With `bias == 0` the origin sits exactly
/// on the surface and the `(EPS, max_t)` open-interval guard in
/// `ray_hit_distance` still rejects the self-hit at `t ~= 0`.
///
/// # Layout contracts
///
/// - `Tri` must match `umber_bake::thickness`'s private `GpuTriangle`
///   byte-for-byte (four `vec4<f32>`s, identical to `AO_BAKE_SHADER`'s `Tri`
///   — the same brute-force triangle list, no BVH, same perf budget of
///   `texels * rays * triangle_count`; see [`AO_BAKE_SHADER`]'s doc comment).
/// - `ThicknessParams` must match `umber_bake::thickness`'s private
///   `ThicknessUniform` byte-for-byte (`rays`, `max_distance`, `bias`,
///   `tri_count` — four 4-byte scalars, 16 bytes total, already a multiple
///   of WGSL's 16-byte uniform-struct alignment, so no padding field is
///   needed).
/// - `position_tex`/`normal_tex` are the two `Rgba32Float` outputs of
///   `POSITION_BAKE_SHADER`'s `cs_main`, bound read-only and sampled with
///   `textureLoad` at integer texel coordinates (no sampler, no filtering).
///   Binding numbers (`0/1/2` plus `4/5`) deliberately mirror
///   `AO_BAKE_SHADER::cs_main_from_position`'s set (which skips `3`, the
///   plane-path `dims` uniform this mesh-fed pass has no use for) so the
///   two raycast passes stay grep-comparable.
/// - `thickness_tex` is a write-only `Rgba8Unorm` storage texture (core
///   WebGPU, no device feature — the same reason [`AO_BAKE_SHADER`] uses
///   `write`, not `read_write`).
///
/// # Dispatch + workgroup shape
///
/// One workgroup (`@workgroup_size(1)`) per texel, dispatched as
/// `dispatch_workgroups(width, height, 1)`, so `workgroup_id.xy` is directly
/// the texel coordinate into all three textures — the same "one workgroup
/// per texel" shape as `AO_BAKE_SHADER::cs_main_from_position`, minus the
/// 64-invocation fan-out. A 64-wide workgroup with an atomic counter works
/// for AO's hit *count* (`atomic<u32>`), but a minimum *distance* is a float
/// with no atomic-min in core WGSL; fanning out would need a shared-memory
/// array plus a manual reduction for no real win (per-texel work here is
/// `rays * tri_count` brute-force tests, trivially serial at this slice's
/// mesh-size budget). Each texel's ray loop is therefore serial, and the
/// output stays deterministic texel-to-texel like AO's fixed sample set.
pub const THICKNESS_BAKE_SHADER: &str = r#"
struct Tri {
    v0: vec4<f32>,
    v1: vec4<f32>,
    v2: vec4<f32>,
    normal: vec4<f32>,
};

struct ThicknessParams {
    rays: u32,
    max_distance: f32,
    bias: f32,
    tri_count: u32,
};

@group(0) @binding(0) var<storage, read> triangles: array<Tri>;
@group(0) @binding(1) var thickness_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: ThicknessParams;
@group(0) @binding(4) var position_tex: texture_2d<f32>;
@group(0) @binding(5) var normal_tex: texture_2d<f32>;

const PI: f32 = 3.14159265358979;
const GOLDEN_CONJ: f32 = 0.6180339887498949;
const EPS: f32 = 1e-6;

/// Deterministic hemisphere direction for ray `i` of `n`, in a local frame
/// where +Z is the hemisphere pole: `cos(theta)` is stratified evenly
/// across `[0, 1)` (uniform over solid angle) and `phi` walks the golden
/// angle. Verbatim the same sample set as `AO_BAKE_SHADER::hemisphere_sample`
/// (fixed, not per-texel jittered) so thickness inherits AO's determinism;
/// the caller negates the pole (`- local_dir.z * normal`) to point the
/// hemisphere into the mesh.
fn hemisphere_sample(i: u32, n: u32) -> vec3<f32> {
    let nf = max(f32(n), 1.0);
    let cos_theta = 1.0 - (f32(i) + 0.5) / nf;
    let sin_theta = sqrt(max(0.0, 1.0 - cos_theta * cos_theta));
    let phi = 2.0 * PI * fract(f32(i) * GOLDEN_CONJ);
    return vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), cos_theta);
}

/// Möller–Trumbore ray-triangle intersection returning the hit distance.
/// Double-sided (no backface culling, matching `AO_BAKE_SHADER`). Returns the
/// hit parameter `t` iff it lands in the open interval `(EPS, max_t`,
/// otherwise `-1.0` — the distance-returning analog of AO's boolean
/// `hits_triangle`, so the caller can keep the minimum across rays.
fn ray_hit_distance(
    orig: vec3<f32>,
    dir: vec3<f32>,
    v0: vec3<f32>,
    v1: vec3<f32>,
    v2: vec3<f32>,
    max_t: f32,
) -> f32 {
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let h = cross(dir, e2);
    let a = dot(e1, h);
    if (abs(a) < EPS) {
        return -1.0;
    }
    let f = 1.0 / a;
    let s = orig - v0;
    let u = f * dot(s, h);
    if (u < 0.0 || u > 1.0) {
        return -1.0;
    }
    let q = cross(s, e1);
    let v = f * dot(dir, q);
    if (v < 0.0 || u + v > 1.0) {
        return -1.0;
    }
    let t = f * dot(e2, q);
    if (t > EPS && t < max_t) {
        return t;
    }
    return -1.0;
}

struct Basis {
    b1: vec3<f32>,
    b2: vec3<f32>,
};

/// Branchless tangent-frame construction from a unit normal (Duff et al.,
/// "Building an Orthonormal Basis, Revisited", JCGT 2017). Verbatim the same
/// frame as `AO_BAKE_SHADER::orthonormal_basis`: any orthonormal frame works
/// (thickness, like AO, is isotropic in `phi`), this one was picked for being
/// branchless and stable at the south pole.
fn orthonormal_basis(n: vec3<f32>) -> Basis {
    let sign_z = select(-1.0, 1.0, n.z >= 0.0);
    let a = -1.0 / (sign_z + n.z);
    let b = n.x * n.y * a;
    return Basis(
        vec3<f32>(1.0 + sign_z * n.x * n.x * a, sign_z * b, -sign_z * n.x),
        vec3<f32>(b, sign_z + n.y * n.y * a, -n.y),
    );
}

/// Mesh-fed thickness: per-texel ray origin and tangent frame come from
/// `position_tex`/`normal_tex` (the two outputs of `POSITION_BAKE_SHADER`'s
/// `cs_main`). `position_tex`'s alpha channel is the position pass's coverage
/// flag: `0.0` means no UV triangle covered this texel, so there is no surface
/// to cast inward from — the texel writes `(0, 0, 0, 0)` with no raycast.
/// Otherwise every ray is cast around the *negative* normal, the minimum hit
/// distance across the whole ray set is normalized by `max_distance`, and the
/// result is written grayscale with full coverage alpha. A texel whose rays
/// all miss writes `1.0` (no opposite face within range reads as maximally
/// thick — see this constant's doc comment).
@compute @workgroup_size(1)
fn cs_main(@builtin(workgroup_id) workgroup_id: vec3<u32>) {
    let coord = vec2<i32>(workgroup_id.xy);
    let sample = textureLoad(position_tex, coord, 0);
    if (sample.w <= 0.5) {
        textureStore(thickness_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    let surface_pos = sample.xyz;
    let normal = normalize(textureLoad(normal_tex, coord, 0).xyz);
    let basis = orthonormal_basis(normal);
    let ray_origin = surface_pos - params.bias * normal;

    var min_dist = params.max_distance;
    let n = params.rays;
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        let local_dir = hemisphere_sample(i, n);
        let dir = normalize(
            local_dir.x * basis.b1 + local_dir.y * basis.b2 - local_dir.z * normal
        );
        for (var t: u32 = 0u; t < params.tri_count; t = t + 1u) {
            let tri = triangles[t];
            let hit = ray_hit_distance(
                ray_origin, dir, tri.v0.xyz, tri.v1.xyz, tri.v2.xyz, params.max_distance
            );
            if (hit > 0.0 && hit < min_dist) {
                min_dist = hit;
            }
        }
    }

    let range = max(params.max_distance, EPS);
    let thickness = clamp(min_dist / range, 0.0, 1.0);
    textureStore(thickness_tex, coord, vec4<f32>(thickness, thickness, thickness, 1.0));
}
"#;

/// UV-padding dilation (post-bake island-margin expansion): one workgroup
/// per output texel, spreading island-edge colors outward into uncovered
/// texels so exported maps have no transparent seams.
///
/// # Algorithm (one dilation step per dispatch)
///
/// Per texel, from the read-only input map `src_tex` into the write-only
/// output `dst_tex`:
///
/// ```text
/// if src.a > 0: copy through unchanged
/// else: scan the 8 neighbors; among covered neighbors pick the one with
///       the HIGHEST coverage (alpha) value, ties keep the first found;
///       write its rgb with alpha 1.0 — or (0,0,0,0) if no neighbor is covered.
/// ```
///
/// `N` steps of padding = `N` ping-pong dispatches driven from Rust (see
/// `umber_bake::dilation::dilate_map`): each step reads the previous step's
/// output, so one ring of texels is claimed per dispatch and the covered
/// front advances exactly one texel per iteration.
///
/// # Why the dilated alpha must normalize to 1.0 (chain propagation)
///
/// A newly-dilated texel must be indistinguishable from an originally
/// covered texel on the *next* step (`a = 1.0`, i.e. byte `255`, both) or
/// propagation stalls after one ring: the next pass's "covered" test is
/// `a > 0`, and its donor ranking compares coverage values, so writing
/// anything less than full coverage would make second-ring texels rank
/// below (or read as uncovered next to) the front and the wave would die
/// out. The donor's rgb is copied verbatim — no blending, no falloff — so
/// color propagates unchanged no matter how many rings it travels.
///
/// # Layout contracts
///
/// - `src_tex` is a read-only `texture_2d<f32>` over an `Rgba8Unorm`
///   texture, sampled with `textureLoad` at integer texel coordinates (no
///   sampler, no filtering — filtering would smear island colors across UV
///   seams, the same reason `CURVATURE_BAKE_SHADER` uses `textureLoad`). The
///   Rust side must create the source texture with
///   `TEXTURE_BINDING` usage; the `BakeTarget` textures it ping-pongs
///   between already carry that flag via `PaintTarget`.
/// - `dst_tex` is a write-only `Rgba8Unorm` storage texture (core WebGPU,
///   no device feature — the same reason [`AO_BAKE_SHADER`] uses `write`,
///   not `read_write`).
/// - `dims` is a `vec2<u32>` uniform holding the target width/height, used
///   only to clamp the 8-neighborhood against the texture edges (no wrap).
/// - No other state: no triangle buffer, no position/normal inputs.
///
/// # Dispatch + workgroup shape
///
/// One workgroup (`@workgroup_size(1)`) per texel, dispatched as
/// `dispatch_workgroups(width, height, 1)`, so `workgroup_id.xy` is directly
/// the texel coordinate into both textures — the same "one workgroup per
/// texel" shape as `AO_BAKE_SHADER::cs_main_from_position` and the
/// curvature/thickness passes, minus any ray loop (per-texel work here is
/// nine `textureLoad`s, trivially serial).
///
/// # Known artifact: thin diagonal streaks from first-found tie-breaking
///
/// With binary coverage (alpha `0` or `1` — everything the bake passes and
/// this pass itself produce), every covered neighbor ties at `1.0` and the
/// strict-`>` comparison keeps the first in scan order (top-left row-major:
/// `(-1,-1)` first, `(1,1)` last). Along diagonal fronts this biases donor
/// choice toward the top-left, leaving faint diagonal streaks in the padded
/// margin where two fronts meet. Acceptable for this slice — Substance's
/// dilator shows the same streaking — and confined to the margin (covered
/// texels copy through untouched, so source pixels are never altered).
pub const DILATE_BAKE_SHADER: &str = r#"
struct DilateDims {
    dims: vec2<u32>,
};

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var dst_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> dilate_dims: DilateDims;

@compute @workgroup_size(1)
fn cs_main(@builtin(workgroup_id) workgroup_id: vec3<u32>) {
    let coord = vec2<i32>(workgroup_id.xy);
    let center = textureLoad(src_tex, coord, 0);
    if (center.w > 0.0) {
        textureStore(dst_tex, coord, center);
        return;
    }

    let dims = vec2<i32>(dilate_dims.dims);
    var best_rgb = vec3<f32>(0.0, 0.0, 0.0);
    var best_a = 0.0;
    for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
        for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
            if (dx == 0 && dy == 0) {
                continue;
            }
            let nc = coord + vec2<i32>(dx, dy);
            if (nc.x < 0 || nc.y < 0 || nc.x >= dims.x || nc.y >= dims.y) {
                continue;
            }
            let s = textureLoad(src_tex, nc, 0);
            // Strict `>`: ties keep the first donor found in scan order
            // (top-left row-major), giving deterministic output — see this
            // constant's "diagonal streaks" doc comment.
            if (s.w > 0.0 && s.w > best_a) {
                best_a = s.w;
                best_rgb = s.xyz;
            }
        }
    }

    if (best_a > 0.0) {
        // Full coverage alpha: the next ping-pong step must read this
        // texel as covered as if it were original island (see this
        // constant's "chain propagation" doc comment).
        textureStore(dst_tex, coord, vec4<f32>(best_rgb, 1.0));
    } else {
        textureStore(dst_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
    }
}
"#;

/// Tangent-space normal baking from a position/normal map: one workgroup
/// per output texel, transforming each covered texel's world normal into a
/// screen-space tangent frame built from the position map's own
/// x-derivative.
///
/// # Screen-space tangent frame (no explicit UVs)
///
/// Per covered texel:
///
/// ```text
/// dpdx = central (both x-neighbors covered) or one-sided (island border)
///        position difference; uncovered/out-of-bounds neighbors are skipped,
///        never zero-filled (a hole must not tilt a neighboring frame)
/// T = normalize(dpdx - N * dot(N, dpdx))   // Gram-Schmidt against N
/// B = cross(N, T)
/// tangent_normal = (dot(T, n), dot(B, n), dot(N, n))  // transpose(TBN) * n
/// ```
///
/// where `N`/`n` is the position pass's per-texel face normal
/// (`normal_tex`, normalized) — the only normal this slice has, so the
/// world normal being transformed and the frame's axis coincide (a flat
/// facet correctly bakes `(0, 0, 1)`; smooth/interpolated normals plug into
/// `n` without touching the frame code — see "Wave-4 refinement" below).
/// A texel with no covered x-neighbor (isolated single-texel island) or a
/// near-zero gradient falls back to the world axis least aligned with `N`,
/// orthogonalized the same way, so `T` is never a normalized zero vector.
///
/// # UV-alignment assumption (matches Substance's default when UVs are axis-aligned)
///
/// Texel `+x` is `+u`: the position pass maps texel centers through
/// `u = (x + 0.5) / width` (see `POSITION_BAKE_SHADER`'s doc comment), so
/// `dP/dx` points along the surface's `+u` direction and `T` is the
/// `+u` tangent — exactly Substance Painter's default tangent frame on
/// meshes whose UV islands are axis-aligned to the baked surface. `B =
/// cross(N, T)` then points along `+v` on such meshes (not along texel
/// `+y`, which is `-v` under the position pass's `(1 - v)` flip), keeping
/// green "up" in UV space per the OpenGL convention below. On rotated UV
/// islands the frame twists with the screen axes instead of the UVs —
/// the known screen-space limitation this slice accepts (same class of
/// artifact as `CURVATURE_BAKE_SHADER`'s UV-seam blindness).
///
/// # Wave-4 refinement plan (per-texel UV-derivative TBN)
///
/// Once the position pass exports per-texel UVs in a channel, replace the
/// `dpdx`-only construction with the standard UV-derivative frame: solve
/// `dP/du`, `dP/dv` from neighbor differences (`dp = dP/du * du + dP/dv *
/// dv` over two covered neighbors), `T = normalize(dP/du - N * dot(N,
/// dP/du))`, `B` from `dP/dv` with a `cross(N, T)`-handedness check
/// against the UV winding. The `transpose(TBN) * n` transform and
/// the encoding below stay unchanged; only the `T`/`B` derivation moves.
///
/// # Encoding + convention (OpenGL default, DirectX flip)
///
/// `rgb = tangent_normal * 0.5 + 0.5`, `a` = coverage (`1.0` covered,
/// `0.0` uncovered). Default is OpenGL (`+Y` up = green up). A nonzero
/// `flip_y` inverts the green channel *after* encoding (`g = 1 - g`) for
/// DirectX (`-Y` up). Uncovered texels write `(0, 0, 0, 0)`,
/// distinguishable from a flat-but-covered texel (`(~0.5, ~0.5, 1, 1)`)
/// by alpha alone — the same alpha convention
/// `AO_BAKE_SHADER::cs_main_from_position` uses, which is why
/// `normal_map::bake_tangent_normal_mesh` returns full RGBA8.
///
/// # Layout contracts
///
/// - `TangentNormalParams` must match `umber_bake::normal_map`'s private
///   `TangentNormalUniform` byte-for-byte (`width`, `height`, `flip_y`,
///   one `u32` pad — 16 bytes, already a multiple of WGSL's 16-byte
///   uniform-struct alignment, so no further padding is needed).
/// - `position_tex`/`normal_tex` are the two `Rgba32Float` outputs of
///   `POSITION_BAKE_SHADER`'s `cs_main`, bound read-only and sampled with
///   `textureLoad` at integer texel coordinates (no sampler, no filtering
///   — filtering would smear normals across UV seams; see
///   `CURVATURE_BAKE_SHADER`'s doc comment). `position_tex`'s alpha is
///   the position pass's coverage flag: `0.0` means no UV triangle covered
///   that texel.
/// - `out_tex` is a write-only `Rgba8Unorm` storage texture (core WebGPU,
///   no device feature — the same reason [`AO_BAKE_SHADER`] uses `write`,
///   not `read_write`).
///
/// # Dispatch + edge behavior
///
/// One workgroup (`@workgroup_size(1)`) per texel, dispatched as
/// `dispatch_workgroups(width, height, 1)`, so `workgroup_id.xy` is
/// directly the texel coordinate into all three textures — the same
/// "one workgroup per texel" shape as `CURVATURE_BAKE_SHADER`, minus any
/// ray loop (per-texel work here is a handful of `textureLoad`s,
/// trivially serial).
pub const TANGENT_NORMAL_BAKE_SHADER: &str = r#"
struct TangentNormalParams {
    width: u32,
    height: u32,
    flip_y: u32,
    _pad: u32,
};

@group(0) @binding(0) var out_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(1) var<uniform> params: TangentNormalParams;
@group(0) @binding(2) var position_tex: texture_2d<f32>;
@group(0) @binding(3) var normal_tex: texture_2d<f32>;

const TN_EPS: f32 = 1e-6;

@compute @workgroup_size(1)
fn cs_main(@builtin(workgroup_id) workgroup_id: vec3<u32>) {
    let coord = vec2<i32>(workgroup_id.xy);
    let center = textureLoad(position_tex, coord, 0);
    if (center.w <= 0.5) {
        textureStore(out_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    let p0 = center.xyz;
    let n_axis = normalize(textureLoad(normal_tex, coord, 0).xyz);
    let dims = vec2<i32>(i32(params.width), i32(params.height));

    // Screen-space dP/dx from the covered x-neighbors: central difference
    // when both sides are covered, one-sided at island borders. Out-of-
    // bounds and uncovered neighbors are skipped, not zero-filled (a hole
    // must not tilt a neighboring frame) — the same rule
    // CURVATURE_BAKE_SHADER uses for its 4-neighborhood.
    var has_px = false;
    var has_nx = false;
    var p_px = vec3<f32>(0.0, 0.0, 0.0);
    var p_nx = vec3<f32>(0.0, 0.0, 0.0);
    if (coord.x + 1 < dims.x) {
        let s = textureLoad(position_tex, coord + vec2<i32>(1, 0), 0);
        if (s.w > 0.5) {
            p_px = s.xyz;
            has_px = true;
        }
    }
    if (coord.x - 1 >= 0) {
        let s = textureLoad(position_tex, coord + vec2<i32>(-1, 0), 0);
        if (s.w > 0.5) {
            p_nx = s.xyz;
            has_nx = true;
        }
    }
    var dpdx = vec3<f32>(0.0, 0.0, 0.0);
    var has_dpdx = false;
    if (has_px && has_nx) {
        dpdx = (p_px - p_nx) * 0.5;
        has_dpdx = true;
    } else if (has_px) {
        dpdx = p_px - p0;
        has_dpdx = true;
    } else if (has_nx) {
        dpdx = p0 - p_nx;
        has_dpdx = true;
    }

    // Gram-Schmidt orthogonalization of the +u tangent against N. An
    // isolated texel (no covered x-neighbor) or a near-zero gradient falls
    // back to the world axis least aligned with N, orthogonalized the same
    // way, so T is never a normalized zero vector.
    var t = vec3<f32>(1.0, 0.0, 0.0);
    if (has_dpdx) {
        t = dpdx - n_axis * dot(n_axis, dpdx);
    }
    var t_len = length(t);
    if (!has_dpdx || t_len < TN_EPS) {
        var axis = vec3<f32>(1.0, 0.0, 0.0);
        if (abs(n_axis.x) > 0.9) {
            axis = vec3<f32>(0.0, 1.0, 0.0);
        }
        t = axis - n_axis * dot(n_axis, axis);
        t_len = length(t);
    }
    let T = t / max(t_len, TN_EPS);
    let B = cross(n_axis, T);

    // World normal into the frame: transpose(TBN) * n. This slice's only
    // normal source is the position pass's face normal, so n == n_axis and
    // a flat facet bakes (0, 0, 1) by construction; a wave-4 smooth-normal
    // input plugs into `n` here without touching the frame above.
    let n = n_axis;
    var tn = vec3<f32>(dot(T, n), dot(B, n), dot(n_axis, n));
    tn = clamp(tn, vec3<f32>(-1.0, -1.0, -1.0), vec3<f32>(1.0, 1.0, 1.0));
    var rgb = tn * 0.5 + vec3<f32>(0.5, 0.5, 0.5);
    if (params.flip_y != 0u) {
        rgb.y = 1.0 - rgb.y;
    }
    rgb = clamp(rgb, vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0));
    textureStore(out_tex, coord, vec4<f32>(rgb, 1.0));
}
"#;

/// Deterministic per-texel region-ID coloring (ID bake, wave-4 item 5 slice 1):
/// one invocation per output texel, hashing the winning triangle's index with
/// FNV-1a and encoding the hash as the texel color.
///
/// # Where the triangle index comes from (row 156-B kill 5)
///
/// The index is GATHERED from the UV G-buffer's `prim` texture
/// ([`UV_GBUFFER_SHADER`], `R32Uint`, `0xFFFFFFFF` = uncovered), the one
/// GPU mirror of the CPU authority `umber_mesh::UvGBuffer`. This pass used
/// to re-run its own copy of the point-in-triangle solve over the position
/// pass's triangle buffer; that duplicate is gone, so coverage and the
/// winning triangle can no longer drift from fill, export padding or the
/// other bakes.
///
/// A per-texel position hash was considered (hash the covered texel's world-pos
/// bits) and rejected: it yields a different color per texel — gradient noise,
/// useless for region masking — where the design-doc contract
/// (`docs/specs/id-bake-bent-normals-design.md`) wants region-constant colors
/// whose value IS the FNV-1a hash of the region identity.
///
/// # Encoding
///
/// `hash = fnv1a_32(triangle_index as 4 little-endian bytes)`,
/// `rgb = [h & 0xFF, (h >> 8) & 0xFF, (h >> 16) & 0xFF] / 255`, `a = 1.0`.
/// Each hash byte is an exact integer multiple of `1/255`, so the
/// `Rgba8Unorm` store round-trips it byte-exactly. Uncovered texels write
/// `(0, 0, 0, 0)`, distinguishable from any covered texel (alpha 1.0) by
/// alpha alone.
///
/// # Layout contracts
///
/// - `prim_tex`: the G-buffer's `R32Uint` prim texture, `textureLoad`ed.
/// - `out_tex`: write-only `Rgba8Unorm` storage (core WebGPU).
/// - `@workgroup_size(8, 8)`, dispatched `ceil(w/8) x ceil(h/8)`.
pub const ID_BAKE_SHADER: &str = r#"
@group(0) @binding(0) var prim_tex: texture_2d<u32>;
@group(0) @binding(1) var out_tex: texture_storage_2d<rgba8unorm, write>;

const NO_PRIM: u32 = 0xFFFFFFFFu;
const FNV_OFFSET_BASIS: u32 = 2166136261u;
const FNV_PRIME: u32 = 16777619u;

fn fnv1a_step(h: u32, b: u32) -> u32 {
    return (h ^ b) * FNV_PRIME;
}

fn hash_triangle(tri: u32) -> u32 {
    var h = FNV_OFFSET_BASIS;
    h = fnv1a_step(h, tri & 0xFFu);
    h = fnv1a_step(h, (tri >> 8u) & 0xFFu);
    h = fnv1a_step(h, (tri >> 16u) & 0xFFu);
    h = fnv1a_step(h, (tri >> 24u) & 0xFFu);
    return h;
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(out_tex);
    if (gid.x >= dims.x || gid.y >= dims.y) {
        return;
    }
    let coord = vec2<i32>(i32(gid.x), i32(gid.y));
    let prim = textureLoad(prim_tex, coord, 0).x;
    if (prim == NO_PRIM) {
        textureStore(out_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }
    let h = hash_triangle(prim);
    let rgb = vec3<f32>(
        f32(h & 0xFFu),
        f32((h >> 8u) & 0xFFu),
        f32((h >> 16u) & 0xFFu),
    ) / 255.0;
    textureStore(out_tex, coord, vec4<f32>(rgb, 1.0));
}
"#;

/// High→low bake transfer: per-texel rays from the LOW mesh's UV map into
/// the HIGH mesh's triangle soup (wave-4 item 4, final slice —
/// `docs/specs/high-to-low-transfer-design.md` §"The architecture" point 3).
///
/// # Per-texel ray configuration
///
/// The LOW surface data comes from the EXISTING position pass, bound
/// read-only exactly like the curvature/thickness passes consume it
/// (`POSITION_BAKE_SHADER`'s output contract — `bake_shaders.rs` docs and
/// `umber_bake::position::bake_position_map` agree):
///
/// - `low_pos_tex` (`Rgba32Float`): `rgb` = world-space position, `a` =
///   coverage (`1.0` iff some LOW triangle's UV footprint covers this texel).
/// - `low_normal_tex` (`Rgba32Float`): `xyz` = the LOW face normal (unit),
///   `a` = coverage. The position pass emits this companion texture from the
///   same dispatch (`position::bake_position_and_normal`) — the ray direction
///   reads it, not a re-derived gradient.
///
/// Per covered texel:
///
/// ```text
/// origin    = low_pos + low_normal * front_offset
/// dir       = -low_normal                       // into the surface, toward the high
/// t_surface = front_offset                      // the LOW surface along the ray (dir is unit)
/// ```
///
/// Uncovered texels (`a <= 0.5`) skip the raycast and write background
/// `(0, 0, 0, 0)` — the same alpha convention
/// `AO_BAKE_SHADER::cs_main_from_position` uses.
///
/// # Intersection: brute force, no GPU BVH
///
/// The HIGH triangle buffer is walked flat, one Möller–Trumbore test per
/// triangle, keeping the nearest hit — the `PlaneDesc` bakes' flat-brute-force
/// precedent the design doc contracts (a GPU BVH is the deferred perf row;
/// the CPU BVH in `umber_mesh::bvh` stays CPU-side for callers). The test
/// mirrors `umber_mesh::raycast::ray_intersect`'s ground-truth semantics
/// (see also `bvh::tri_hit`): double-sided (no culling), `1e-8` parallel
/// epsilon, `u in 0..=1` / `v >= 0` / `u + v <= 1` barycentric bounds, `t >
/// 1e-5` near-clip. Ties (two HIGH triangles reporting equal `t`, e.g. a
/// shared edge) resolve to the FIRST triangle in buffer order (strict
/// `<` replacement) — deterministic, documented here because the CPU BVH
/// instead tie-breaks to the smallest triangle index.
///
/// The loop is bounded above by `t_surface + back_distance`: hits past the
/// far clamp can never validate, so they are culled inside the intersection
/// test itself (the "bounded early-out via the clamp distances").
///
/// # Clamp gate (mirrors `umber_mesh::bake_support::TransferClamps`)
///
/// A hit at ray parameter `hit_t` is valid iff
/// `t_surface - front_distance <= hit_t <= t_surface + back_distance`
/// (`TransferClamps::clamps_hit`). One nuance: the far bound inside the
/// intersection loop is STRICT (`t < max_t`, the Möller–Trumbore open
/// interval), while `clamps_hit` is inclusive on both sides — a hit landing
/// bit-exactly on `t_surface + back_distance` is culled here but kept by the
/// CPU gate. Only exact-boundary hits can observe the difference; every
/// tested configuration keeps hits well clear of the bounds.
///
/// # Outputs (one shader, uniform-selected via `map_mode`)
///
/// - `0 = height`: `h = (hit_t - t_surface + front_distance) /
///   (front_distance + back_distance)`, written grayscale with `a = 1`.
///   Note the centering: a hit exactly ON the low surface reads
///   `front_distance / (front_distance + back_distance)` — `0.5` only when
///   `front_distance == back_distance`. (An early brief draft pinned `0.5`
///   for asymmetric clamps; the shader implements the design doc's
///   "normalized by `front_distance + back_distance`" formula, and the
///   `umber_bake::transfer` tests derive their exact bytes from it.)
/// - `1 = world normal`: the HIT triangle's face normal (`tri.normal`,
///   normalized), encoded `rgb = n * 0.5 + 0.5`, `a = 1`. This is the
///   "object/world space" normal-space option requirements §3 names.
/// - `2 = tangent normal`: the HIT normal transformed into the LOW's
///   per-texel UV-derivative TBN frame (`transfer_tbn` below), encoded
///   `rgb = transpose(TBN) * n * 0.5 + 0.5`, `a = 1` — the Mikktspace-style
///   tangent-space fold (green = +Y up, OpenGL).
///   WORLD-space v1: the HIGH normal is written as-is, NOT transformed into
///   the LOW's tangent frame — per-texel UV-derivative TBN is the deferred
///   fold (see `TANGENT_NORMAL_BAKE_SHADER`'s screen-space-frame discussion
///   for why a correct tangent frame needs per-texel UV derivatives this
///   pass does not bind). Interpolated vertex normals are likewise deferred:
///   like every other baker here, the triangle buffer carries the recomputed
///   FACE normal (always available, even for UV-less IMPORTS without vertex
///   normals), so a faceted HIGH bakes faceted normals.
///
/// A validated hit outside `map_mode ∈ {0, 1, 2}` cannot occur (the Rust side
/// only ever writes `0`/`1`/`2`); misses write background `(0, 0, 0, 0)`.
///
/// # The cage does NOT enter v1's GPU path
///
/// `Cage::lerp_at` needs per-vertex offsets interpolated mid-shader, which
/// needs a cage buffer this binding set does not carry. Cage support lands
/// when the app-wiring slice adds that buffer; the FRONT/BACK clamps DO
/// enter v1 (see above).
///
/// # Layout contracts
///
/// - `Tri` must match `umber_bake::transfer`'s private `GpuTriangle`
///   byte-for-byte (four `vec4<f32>`s: three HIGH vertex positions, then the
///   HIGH face normal — byte-identical to `AO_BAKE_SHADER`'s/`THICKNESS_BAKE_SHADER`'s
///   `Tri`, NOT `POSITION_BAKE_SHADER`'s 80-byte `PosTri`: the HIGH mesh
///   needs no UVs per the design doc ("no UVs needed"), and the `PosTri`
///   builder ERRORS on missing UVs, so the UV-free AO-style layout is the
///   fitting reuse — only positions/normal are consumed here anyway).
/// - `TransferParams` must match `umber_bake::transfer`'s private
///   `TransferUniform` byte-for-byte (`front_distance`, `back_distance`,
///   `front_offset`, `high_tri_count`, `map_mode`, `width`, `height`, one
///   `u32` pad — 32 bytes, already a multiple of WGSL's 16-byte
///   uniform-struct alignment). `width`/`height` document the target the
///   dispatch was sized for; the shader itself needs no dims uniform
///   (`workgroup_id.xy` IS the texel coordinate — the
///   `cs_main_from_position` convention).
/// - `low_pos_tex`/`low_normal_tex` are the two `Rgba32Float` outputs of
///   `POSITION_BAKE_SHADER`'s `cs_main`, bound read-only and sampled with
///   `textureLoad` (no sampler, no filtering).
/// - `out_tex` is a write-only `Rgba8Unorm` storage texture (core WebGPU,
///   no device feature — the same reason [`AO_BAKE_SHADER`] uses `write`,
///   not `read_write`).
///
/// # Dispatch + workgroup shape
///
/// One workgroup (`@workgroup_size(1)`) per texel, dispatched as
/// `dispatch_workgroups(width, height, 1)` — the same shape as
/// `CURVATURE_BAKE_SHADER`, minus the neighborhood (per-texel work here is
/// one serial brute-force ray loop; a minimum-*distance* plus nearest-hit
/// record has no atomic form in core WGSL, so — like thickness — the loop
/// stays serial and the output stays deterministic texel-to-texel).
pub const TRANSFER_BAKE_SHADER: &str = r#"
struct Tri {
    v0: vec4<f32>,
    v1: vec4<f32>,
    v2: vec4<f32>,
    normal: vec4<f32>,
};

struct TransferParams {
    front_distance: f32,
    back_distance: f32,
    front_offset: f32,
    high_tri_count: u32,
    map_mode: u32,
    width: u32,
    height: u32,
    _pad: u32,
};

@group(0) @binding(0) var<storage, read> high_tris: array<Tri>;
@group(0) @binding(1) var out_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: TransferParams;
@group(0) @binding(3) var low_pos_tex: texture_2d<f32>;
@group(0) @binding(4) var low_normal_tex: texture_2d<f32>;

// Möller–Trumbore constants mirroring `umber_mesh::raycast::ray_intersect`
// (see also `umber_mesh::bvh::tri_hit`): 1e-8 parallel epsilon, 1e-5
// near-clip. (The AO/thickness shaders use a looser 1e-6 single EPS for
// both roles; this pass tracks the raycast ground truth instead, since the
// transfer tests derive exact hit parameters from it.)
const MT_PARALLEL_EPS: f32 = 1e-8;
const MT_T_EPS: f32 = 1e-5;

/// Möller–Trumbore ray-triangle intersection returning the hit distance.
/// Double-sided (no backface culling, matching `AO_BAKE_SHADER`). Returns the
/// hit parameter `t` iff it lands in the open interval `(MT_T_EPS, max_t)`,
/// otherwise `-1.0` — the distance-returning shape of
/// `THICKNESS_BAKE_SHADER::ray_hit_distance`, so the caller can keep the
/// nearest hit across the HIGH triangle list.
fn ray_hit_distance(
    orig: vec3<f32>,
    dir: vec3<f32>,
    v0: vec3<f32>,
    v1: vec3<f32>,
    v2: vec3<f32>,
    max_t: f32,
) -> f32 {
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let h = cross(dir, e2);
    let a = dot(e1, h);
    if (abs(a) < MT_PARALLEL_EPS) {
        return -1.0;
    }
    let f = 1.0 / a;
    let s = orig - v0;
    let u = f * dot(s, h);
    if (u < 0.0 || u > 1.0) {
        return -1.0;
    }
    let q = cross(s, e1);
    let v = f * dot(dir, q);
    if (v < 0.0 || u + v > 1.0) {
        return -1.0;
    }
    let t = f * dot(e2, q);
    if (t > MT_T_EPS && t < max_t) {
        return t;
    }
    return -1.0;
}

/// Mikktspace-style per-texel UV-derivative frame; green = +Y up (OpenGL/Mikktspace).
struct TbnFrame {
    T: vec3<f32>,
    B: vec3<f32>,
};

fn transfer_tbn(coord: vec2<i32>, p0: vec3<f32>, N: vec3<f32>) -> TbnFrame {
    let dims = vec2<i32>(i32(params.width), i32(params.height));
    var p_px = vec3<f32>(0.0, 0.0, 0.0);
    var p_nx = vec3<f32>(0.0, 0.0, 0.0);
    var p_py = vec3<f32>(0.0, 0.0, 0.0);
    var p_ny = vec3<f32>(0.0, 0.0, 0.0);
    var has_px = false;
    var has_nx = false;
    var has_py = false;
    var has_ny = false;
    var len_px = 0.0;
    var len_nx = 0.0;
    var len_py = 0.0;
    var len_ny = 0.0;
    if (coord.x + 1 < dims.x) {
        let s = textureLoad(low_pos_tex, coord + vec2<i32>(1, 0), 0);
        if (s.w > 0.5) { p_px = s.xyz; has_px = true; len_px = length(s.xyz - p0); }
    }
    if (coord.x - 1 >= 0) {
        let s = textureLoad(low_pos_tex, coord + vec2<i32>(-1, 0), 0);
        if (s.w > 0.5) { p_nx = s.xyz; has_nx = true; len_nx = length(s.xyz - p0); }
    }
    if (coord.y + 1 < dims.y) {
        let s = textureLoad(low_pos_tex, coord + vec2<i32>(0, 1), 0);
        if (s.w > 0.5) { p_py = s.xyz; has_py = true; len_py = length(s.xyz - p0); }
    }
    if (coord.y - 1 >= 0) {
        let s = textureLoad(low_pos_tex, coord + vec2<i32>(0, -1), 0);
        if (s.w > 0.5) { p_ny = s.xyz; has_ny = true; len_ny = length(s.xyz - p0); }
    }
    // Local texel scale = smallest valid neighbor jump; a jump > 4x it
    // crossed a UV seam, so that neighbor is skipped (opposite side used).
    var scale = 1e30;
    if (has_px) { scale = min(scale, len_px); }
    if (has_nx) { scale = min(scale, len_nx); }
    if (has_py) { scale = min(scale, len_py); }
    if (has_ny) { scale = min(scale, len_ny); }
    scale = max(scale, 1e-6);
    if (has_px && len_px > 4.0 * scale) { has_px = false; }
    if (has_nx && len_nx > 4.0 * scale) { has_nx = false; }
    if (has_py && len_py > 4.0 * scale) { has_py = false; }
    if (has_ny && len_ny > 4.0 * scale) { has_ny = false; }
    // Texel +x is +u; texel +y is -v (position pass's `(1 - v)` flip), so
    // the v-derivative raw vector is (ny - py), not (py - ny).
    var has_du = false;
    var has_dv = false;
    var du = vec3<f32>(0.0, 0.0, 0.0);
    var dv = vec3<f32>(0.0, 0.0, 0.0);
    if (has_px && has_nx) { du = (p_px - p_nx) * 0.5; has_du = true; }
    else if (has_px) { du = p_px - p0; has_du = true; }
    else if (has_nx) { du = p0 - p_nx; has_du = true; }
    if (has_py && has_ny) { dv = (p_ny - p_py) * 0.5; has_dv = true; }
    else if (has_ny) { dv = p_ny - p0; has_dv = true; }
    else if (has_py) { dv = p0 - p_py; has_dv = true; }
    // Zero-derivative texel: fall back to whichever derivative axis is
    // nonzero, else the (1,0,0) Gram-Schmidt fallback below.
    var t_raw = vec3<f32>(1.0, 0.0, 0.0);
    if (has_du && length(du) > 1e-6) { t_raw = du; }
    else if (has_dv && length(dv) > 1e-6) { t_raw = dv; }
    var t = t_raw - N * dot(N, t_raw);
    var tl = length(t);
    if (!(tl > 1e-6)) {
        t = vec3<f32>(1.0, 0.0, 0.0) - N * dot(N, vec3<f32>(1.0, 0.0, 0.0));
        tl = length(t);
    }
    let T = t / max(tl, 1e-6);
    return TbnFrame(T, cross(N, T));
}

/// Mesh-fed high→low transfer: per-texel ray origin and frame come from
/// `low_pos_tex`/`low_normal_tex` (the two outputs of `POSITION_BAKE_SHADER`'s
/// `cs_main`, baked from the LOW mesh). `low_pos_tex`'s alpha is the position
/// pass's coverage flag: `<= 0.5` means no LOW UV triangle covered this texel,
/// so there is no surface to cast from — the texel writes `(0, 0, 0, 0)`.
/// Otherwise one ray (`origin = low_pos + n * front_offset` along `-n`) is
/// brute-forced against the HIGH triangle list, gated by the front/back
/// clamps, and the uniform-selected map is written (see this constant's doc
/// comment for the ray math, the gate, and both encodings).
@compute @workgroup_size(1)
fn cs_main(@builtin(workgroup_id) workgroup_id: vec3<u32>) {
    let coord = vec2<i32>(workgroup_id.xy);
    let sample = textureLoad(low_pos_tex, coord, 0);
    if (sample.w <= 0.5) {
        textureStore(out_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    let low_pos = sample.xyz;
    let n = normalize(textureLoad(low_normal_tex, coord, 0).xyz);
    let origin = low_pos + n * params.front_offset;
    let dir = -n;
    // The LOW surface sits `front_offset` behind the origin along the unit
    // ray (origin = surface + n * front_offset, dir = -n), so the surface
    // parameter is exactly `front_offset`.
    let t_surface = params.front_offset;
    // Far-clamp early-out: hits at or past `t_surface + back_distance` can
    // never validate (see this constant's "Clamp gate" doc comment for the
    // strict-vs-inclusive nuance at the exact boundary).
    let max_t = t_surface + params.back_distance;

    var best_t = max_t;
    var best_n = vec3<f32>(0.0, 0.0, 1.0);
    var found = false;
    for (var t: u32 = 0u; t < params.high_tri_count; t = t + 1u) {
        let tri = high_tris[t];
        let hit = ray_hit_distance(
            origin, dir, tri.v0.xyz, tri.v1.xyz, tri.v2.xyz, max_t
        );
        // Strict `<`: ties keep the FIRST triangle in buffer order (see this
        // constant's doc comment — deterministic, unlike an index-agnostic
        // `<=` that would depend on traversal order).
        if (hit > 0.0 && (!found || hit < best_t)) {
            best_t = hit;
            best_n = normalize(tri.normal.xyz);
            found = true;
        }
    }
    // Near-clamp gate (`TransferClamps::clamps_hit`'s lower half; the upper
    // half was already enforced by `max_t` inside the loop).
    if (!found || best_t < t_surface - params.front_distance) {
        textureStore(out_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    if (params.map_mode == 0u) {
        let span = max(params.front_distance + params.back_distance, 1e-6);
        let h = clamp(
            (best_t - t_surface + params.front_distance) / span, 0.0, 1.0
        );
        textureStore(out_tex, coord, vec4<f32>(h, h, h, 1.0));
    } else if (params.map_mode == 1u) {
        let enc = clamp(
            best_n * 0.5 + vec3<f32>(0.5, 0.5, 0.5),
            vec3<f32>(0.0, 0.0, 0.0),
            vec3<f32>(1.0, 1.0, 1.0),
        );
        textureStore(out_tex, coord, vec4<f32>(enc, 1.0));
    } else {
        // Tangent-space hit normal: transpose(TBN) * n, +Y-up green.
        var axis = textureLoad(low_normal_tex, coord, 0).xyz;
        let axis_len = length(axis);
        if (!(axis_len > 1e-6)) { axis = vec3<f32>(0.0, 0.0, 1.0); }
        else { axis = axis / axis_len; }
        let frame = transfer_tbn(coord, low_pos, axis);
        var hn = best_n;
        let hn_len = length(hn);
        if (!(hn_len > 1e-6)) { hn = axis; }
        else { hn = hn / hn_len; }
        var tn = vec3<f32>(dot(frame.T, hn), dot(frame.B, hn), dot(axis, hn));
        tn = clamp(tn, vec3<f32>(-1.0, -1.0, -1.0), vec3<f32>(1.0, 1.0, 1.0));
        var rgb = clamp(
            tn * 0.5 + vec3<f32>(0.5, 0.5, 0.5),
            vec3<f32>(0.0, 0.0, 0.0),
            vec3<f32>(1.0, 1.0, 1.0),
        );
        if (!(abs(rgb.x) <= 1.0)) { rgb.x = 0.5; }
        if (!(abs(rgb.y) <= 1.0)) { rgb.y = 0.5; }
        if (!(abs(rgb.z) <= 1.0)) { rgb.z = 0.5; }
        textureStore(out_tex, coord, vec4<f32>(rgb, 1.0));
    }
}
"#;

/// Skew-deviation baking (§6:57): the per-texel tangent-deviation channel
/// the baker emits alongside the guarded maps, plus the edge-protection
/// guard's shader half.
///
/// # Estimator
///
/// Per covered texel, over the SAME deterministic stratified hemisphere
/// sample set as [`AO_BAKE_SHADER::hemisphere_sample`] (copied verbatim,
/// not re-derived — skew must agree with the AO rays texel-for-texel),
/// steered by the SAME Duff-et-al. `orthonormal_basis` tangent frame from
/// the position pass's per-texel normal:
///
/// ```text
/// bent = normalize(sum of unoccluded ray dirs)   // the bent vector
/// skew = acos(clamp(dot(normal, bent), -1, 1)) / (PI / 2)   // 0..1
/// ```
///
/// Output is grayscale `rgb = skew` (`deviation / 90°`, saturating past a
/// right angle), `a` = coverage (`1.0` covered, `0.0` uncovered) — the
/// encoding `umber_bake::skew::skew_byte` reproduces on the CPU side. A
/// texel with no unoccluded ray (fully occluded: no bent direction
/// exists) writes the geometric normal's encoding of zero skew with
/// `a = 1.0` still covered — skew measures projection quality, not
/// visibility, and a crevice's projection is as valid as a plane's. An
/// uncovered texel (the position pass's coverage flag `<= 0.5`) skips the
/// raycast and writes `(0, 0, 0, 0)`, matching every other baker.
///
/// # Edge protection (the guard in the bake pass)
///
/// `skew_threshold` (same 0..1 encoding: `threshold_deg / 90°`) is the
/// shader half of `umber_bake::skew::apply_edge_protection`'s guard: where
/// `skew > skew_threshold` the pass writes the edge-safe neutral
/// (`vec3(1.0)` — unoccluded white, which never darkens a seam) into
/// `guard_tex` INSTEAD of the guarded value, and the texel counts toward
/// the bake log's skew-clamped warnings. The CPU side applies the same
/// guard after the paint-combined skew (`max(baked, painted)` — the
/// paintable half) so brushed "don't bake here" regions clamp even where
/// the rays measured flat; the shader guard covers the unpainted fast
/// path without a readback round-trip.
///
/// # Layout contracts
///
/// - `Tri` must match `umber_bake::ao`'s private `GpuTriangle`
///   byte-for-byte (four `vec4<f32>`s — the same brute-force triangle
///   list, no BVH, same perf budget of `texels * rays * triangle_count`;
///   see [`AO_BAKE_SHADER`]'s doc comment).
/// - `SkewParams` must match `umber_bake::skew`'s GPU uniform
///   byte-for-byte (`rays`, `max_distance`, `bias`, `tri_count`,
///   `skew_threshold` + three `u32` pads — 32 bytes, a multiple of WGSL's
///   16-byte uniform-struct alignment).
/// - `position_tex`/`normal_tex` are the two `Rgba32Float` outputs of
///   `POSITION_BAKE_SHADER`'s `cs_main`, bound read-only and sampled with
///   `textureLoad` at integer texel coordinates (no sampler, no filtering).
///   Binding numbers (`0/1/2` plus `4/5/6`) mirror
///   `AO_BAKE_SHADER::cs_main_from_position`'s set plus one output, so the
///   raycast passes stay grep-comparable.
/// - `skew_tex` is the deviation output, `guard_tex` the edge-protected
///   copy of the guarded input `guard_src_tex` (both write-only
///   `Rgba8Unorm` storage textures — core WebGPU, no device feature).
///
/// # Dispatch + workgroup shape
///
/// One workgroup (`@workgroup_size(1)`) per texel, dispatched as
/// `dispatch_workgroups(width, height, 1)` — the serial-per-texel shape of
/// `THICKNESS_BAKE_SHADER` (a minimum *distance* has no atomic-min in core
/// WGSL, and neither does a direction *sum*; the bent accumulation here is
/// a plain `vec3` variable, reduced implicitly by the serial ray loop).
pub const SKEW_BAKE_SHADER: &str = r#"
struct Tri {
    v0: vec4<f32>,
    v1: vec4<f32>,
    v2: vec4<f32>,
    normal: vec4<f32>,
};

struct SkewParams {
    rays: u32,
    max_distance: f32,
    bias: f32,
    tri_count: u32,
    skew_threshold: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<storage, read> triangles: array<Tri>;
@group(0) @binding(1) var skew_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: SkewParams;
@group(0) @binding(4) var position_tex: texture_2d<f32>;
@group(0) @binding(5) var normal_tex: texture_2d<f32>;
@group(0) @binding(6) var guard_src_tex: texture_2d<f32>;
@group(0) @binding(7) var guard_tex: texture_storage_2d<rgba8unorm, write>;

const PI: f32 = 3.14159265358979;
const GOLDEN_CONJ: f32 = 0.6180339887498949;
const EPS: f32 = 1e-6;

/// Deterministic hemisphere direction for ray `i` of `n`, in a local frame
/// where +Z is the hemisphere pole. Verbatim the same sample set as
/// `AO_BAKE_SHADER::hemisphere_sample` (fixed, not per-texel jittered) so
/// skew agrees with the AO rays texel-for-texel.
fn hemisphere_sample(i: u32, n: u32) -> vec3<f32> {
    let nf = max(f32(n), 1.0);
    let cos_theta = 1.0 - (f32(i) + 0.5) / nf;
    let sin_theta = sqrt(max(0.0, 1.0 - cos_theta * cos_theta));
    let phi = 2.0 * PI * fract(f32(i) * GOLDEN_CONJ);
    return vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), cos_theta);
}

/// Möller–Trumbore ray-triangle intersection. Double-sided (no backface
/// culling, matching `AO_BAKE_SHADER`). Returns true iff the hit parameter
/// `t` lands in the open interval `(EPS, max_t)`.
fn hits_triangle(
    orig: vec3<f32>,
    dir: vec3<f32>,
    v0: vec3<f32>,
    v1: vec3<f32>,
    v2: vec3<f32>,
    max_t: f32,
) -> bool {
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let h = cross(dir, e2);
    let a = dot(e1, h);
    if (abs(a) < EPS) {
        return false;
    }
    let f = 1.0 / a;
    let s = orig - v0;
    let u = f * dot(s, h);
    if (u < 0.0 || u > 1.0) {
        return false;
    }
    let q = cross(s, e1);
    let v = f * dot(dir, q);
    if (v < 0.0 || u + v > 1.0) {
        return false;
    }
    let t = f * dot(e2, q);
    return t > EPS && t < max_t;
}

struct Basis {
    b1: vec3<f32>,
    b2: vec3<f32>,
};

/// Branchless tangent-frame construction from a unit normal (Duff et al.,
/// "Building an Orthonormal Basis, Revisited", JCGT 2017). Verbatim the same
/// frame as `AO_BAKE_SHADER::orthonormal_basis`.
fn orthonormal_basis(n: vec3<f32>) -> Basis {
    let sign_z = select(-1.0, 1.0, n.z >= 0.0);
    let a = -1.0 / (sign_z + n.z);
    let b = n.x * n.y * a;
    return Basis(
        vec3<f32>(1.0 + sign_z * n.x * n.x * a, sign_z * b, -sign_z * n.x),
        vec3<f32>(b, sign_z + n.y * n.y * a, -n.y),
    );
}

/// Mesh-fed skew: the per-texel ray origin and tangent frame come from
/// `position_tex`/`normal_tex`. Uncovered texels write `(0, 0, 0, 0)` to
/// both outputs with no raycast. Covered texels accumulate the unoccluded
/// ray directions into `bent`, encode `skew = acos(dot(n, bent)) / 90°`
/// into `skew_tex`, and write the edge-protected guarded value into
/// `guard_tex` (neutral white where `skew > skew_threshold`).
@compute @workgroup_size(1)
fn cs_main(@builtin(workgroup_id) workgroup_id: vec3<u32>) {
    let coord = vec2<i32>(workgroup_id.xy);
    let sample = textureLoad(position_tex, coord, 0);
    if (sample.w <= 0.5) {
        textureStore(skew_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        textureStore(guard_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    let surface_pos = sample.xyz;
    let normal = normalize(textureLoad(normal_tex, coord, 0).xyz);
    let basis = orthonormal_basis(normal);
    let ray_origin = surface_pos + params.bias * normal;

    var bent = vec3<f32>(0.0, 0.0, 0.0);
    let n = params.rays;
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        let local_dir = hemisphere_sample(i, n);
        let dir = normalize(
            local_dir.x * basis.b1 + local_dir.y * basis.b2 + local_dir.z * normal
        );
        var hit = false;
        for (var t: u32 = 0u; t < params.tri_count; t = t + 1u) {
            let tri = triangles[t];
            if (hits_triangle(ray_origin, dir, tri.v0.xyz, tri.v1.xyz, tri.v2.xyz, params.max_distance)) {
                hit = true;
                break;
            }
        }
        if (!hit) {
            bent = bent + dir;
        }
    }

    // Fully occluded: no unoccluded direction exists — the projection is
    // still valid, so skew reads 0 (not NaN, not 1).
    var skew = 0.0;
    let bent_len = length(bent);
    if (bent_len > EPS) {
        let cos_dev = clamp(dot(normal, bent / bent_len), -1.0, 1.0);
        skew = acos(cos_dev) / (PI * 0.5);
    }
    skew = clamp(skew, 0.0, 1.0);
    textureStore(skew_tex, coord, vec4<f32>(skew, skew, skew, 1.0));

    // Edge protection: the guard in the bake pass. Past-threshold texels
    // bake the edge-safe neutral instead of the guarded value.
    let guarded = textureLoad(guard_src_tex, coord, 0);
    if (skew > params.skew_threshold) {
        textureStore(guard_tex, coord, vec4<f32>(1.0, 1.0, 1.0, guarded.w));
    } else {
        textureStore(guard_tex, coord, guarded);
    }
}
"#;

/// The UV G-buffer pass (queue row 156-B kill 4): per texel of one UDIM
/// tile, the covering triangle index and its barycentrics, the GPU mirror
/// of the CPU authority `umber_mesh::UvGBuffer`.
///
/// # Contract (identical to the CPU side)
///
/// - Texel center UV: `u = tu + (x + 0.5) / w`, `v = tv + 1 - (y + 0.5) / h`
///   (`tu`, `tv` = the tile's integer offset, carried in `params.tile_off`).
/// - Edge functions and weights `b_i = -e_i / area`, written out term for
///   term in the CPU's order; the signed `area` is computed ONCE on the CPU
///   and uploaded (`uv2.z`) so both sides divide by the same bits.
/// - Inside = every `b_i >= -UV_BARY_EPS` (edge-inclusive, winding-free);
///   degenerate, malformed, non-finite and ILL-CONDITIONED triangles
///   (`|area| / longest_edge² < umber_mesh::uv_gbuffer::UV_COND_EPS`, the
///   W1.1 needle rule) arrive as all-zero records (`area = 0`) and never
///   claim a texel. The conditioning rule lives ONLY on the CPU
///   (`UvTri::from_uvs`); the shader inherits it through that zero record,
///   so it has no WGSL constant.
/// - Overlap: last in index order wins.
///
/// # Outputs
///
/// - `bary_tex` (`Rgba32Float`): `(b0, b1, b2, coverage)`, coverage 1.0 or 0.0.
/// - `prim_tex` (`R32Uint`): triangle index, `0xFFFFFFFF` = uncovered.
///
/// One invocation per texel over the triangle list, staged through
/// workgroup memory in `TRI_CHUNK` batches (each triangle read from global
/// memory once per 8x8 workgroup, the scheme `POSITION_BAKE_SHADER` used
/// before it became a gather). Budget: `MAX_TRIS_PER_BAKE`. Not for the
/// paint hot path.
pub const UV_GBUFFER_SHADER: &str = r#"
struct GbTri {
    uv01: vec4<f32>, // x=uv0.x y=uv0.y z=uv1.x w=uv1.y
    uv2: vec4<f32>,  // x=uv2.x y=uv2.y z=signed UV area (0 = skip) w unused
};

struct GbParams {
    dims: vec2<u32>,
    tri_count: u32,
    _pad: u32,
    tile_off: vec2<f32>,
    _pad2: vec2<f32>,
};

@group(0) @binding(0) var<storage, read> tris: array<GbTri>;
@group(0) @binding(1) var bary_tex: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var prim_tex: texture_storage_2d<r32uint, write>;
@group(0) @binding(3) var<uniform> params: GbParams;

const UV_DET_EPS: f32 = 1e-10;
const UV_BARY_EPS: f32 = 1e-6;
const NO_PRIM: u32 = 0xFFFFFFFFu;
const TILE_AREA: u32 = 64u;
// 256 x 32 B = 8 KiB of workgroup storage: half the 16 KiB a default
// device guarantees (the budget POSITION_BAKE_SHADER's doc measures).
const TRI_CHUNK: u32 = 256u;

var<workgroup> chunk: array<GbTri, TRI_CHUNK>;

@compute @workgroup_size(8, 8, 1)
fn cs_main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    // Every invocation stays in the loop (barriers need uniform control
    // flow); only the final stores are bounds-guarded.
    let in_bounds = gid.x < params.dims.x && gid.y < params.dims.y;
    let w = f32(max(params.dims.x, 1u));
    let h = f32(max(params.dims.y, 1u));
    let p = vec2<f32>(
        params.tile_off.x + (f32(gid.x) + 0.5) / w,
        params.tile_off.y + 1.0 - (f32(gid.y) + 0.5) / h,
    );
    var best: u32 = NO_PRIM;
    var bb = vec3<f32>(0.0, 0.0, 0.0);
    for (var base: u32 = 0u; base < params.tri_count; base = base + TRI_CHUNK) {
        let count = min(params.tri_count - base, TRI_CHUNK);
        for (var i: u32 = local_index; i < count; i = i + TILE_AREA) {
            chunk[i] = tris[base + i];
        }
        workgroupBarrier();
        for (var k: u32 = 0u; k < count; k = k + 1u) {
            let tri = chunk[k];
            let area = tri.uv2.z;
            if (abs(area) < UV_DET_EPS) {
                continue;
            }
            let uv0 = tri.uv01.xy;
            let uv1 = tri.uv01.zw;
            let uv2 = tri.uv2.xy;
            let e0 = (p.x - uv1.x) * (uv2.y - uv1.y) - (p.y - uv1.y) * (uv2.x - uv1.x);
            let e1 = (p.x - uv2.x) * (uv0.y - uv2.y) - (p.y - uv2.y) * (uv0.x - uv2.x);
            let e2 = (p.x - uv0.x) * (uv1.y - uv0.y) - (p.y - uv0.y) * (uv1.x - uv0.x);
            let b = vec3<f32>(-e0 / area, -e1 / area, -e2 / area);
            if (b.x >= -UV_BARY_EPS && b.y >= -UV_BARY_EPS && b.z >= -UV_BARY_EPS) {
                best = base + k;
                bb = b;
            }
        }
        workgroupBarrier();
    }
    if (in_bounds) {
        let coord = vec2<i32>(i32(gid.x), i32(gid.y));
        if (best == NO_PRIM) {
            textureStore(bary_tex, coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        } else {
            textureStore(bary_tex, coord, vec4<f32>(bb, 1.0));
        }
        textureStore(prim_tex, coord, vec4<u32>(best, 0u, 0u, 0u));
    }
}
"#;

#[cfg(test)]
mod uv_eps_parity {
    //! Row 156-B: the WGSL UV solves must quote the same epsilon pair as
    //! the CPU authority `umber_mesh::uv_gbuffer`, so a texel the CPU fill
    //! covers is a texel the GPU bakes cover. A drift fails here, not in
    //! a golden three crates away.
    use super::{ID_BAKE_SHADER, POSITION_BAKE_SHADER, UV_GBUFFER_SHADER};

    fn wgsl_const(src: &str, name: &str) -> f32 {
        let needle = format!("const {name}: f32 = ");
        let start = src
            .find(&needle)
            .unwrap_or_else(|| panic!("{name} missing"))
            + needle.len();
        let end = start + src[start..].find(';').expect("terminated const");
        src[start..end].trim().parse().expect("f32 literal")
    }

    /// The ID bake gathers from the G-buffer and must not carry its own
    /// solve again (row 156-B kill 5 removed duplicate S7).
    #[test]
    fn id_bake_has_no_private_uv_solve() {
        assert!(!ID_BAKE_SHADER.contains("EPS"));
        assert!(ID_BAKE_SHADER.contains("prim_tex"));
    }

    /// Kill 5b: the position bake is a gather too; the G-buffer shader is
    /// now the ONLY WGSL UV solve.
    #[test]
    fn position_bake_has_no_private_uv_solve() {
        assert!(!POSITION_BAKE_SHADER.contains("EPS"));
        assert!(POSITION_BAKE_SHADER.contains("prim_tex"));
        assert!(POSITION_BAKE_SHADER.contains("bary_tex"));
    }

    #[test]
    fn bake_shader_eps_match_the_cpu_uv_gbuffer() {
        let src = UV_GBUFFER_SHADER;
        assert_eq!(wgsl_const(src, "UV_DET_EPS"), umber_mesh::UV_DET_EPS);
        assert_eq!(wgsl_const(src, "UV_BARY_EPS"), umber_mesh::UV_BARY_EPS);
    }
}
