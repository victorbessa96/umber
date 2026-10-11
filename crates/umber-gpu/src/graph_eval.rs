//! GPU node-graph evaluation: [`GraphGpuEvaluator`].
//!
//! This is the §5-P0 row "Graph evaluated on GPU (WGSL compute passes per
//! dirty tile)": the topo-sorted dirty subgraph runs as a sequence of
//! WGSL compute dispatches ([`crate::graph_shaders::GRAPH_EVAL_SHADER`],
//! one pipeline per entry point), one dispatch per (node, tile) pair,
//! with node outputs living in intermediate GPU textures. The CPU engine
//! (`umber_graph::eval`) is untouched and stays the byte-exact reference
//! (see the precision contract in [`crate::graph_shaders`]).
//!
//! # The hybrid split table (v1)
//!
//! Not every node runs on the GPU in v1. Routing ([`GpuRoute::route`]) is
//! total — every `node_def`, registered or not, maps to exactly one path:
//!
//! | `node_def` | v1 path | why |
//! |---|---|---|
//! | `uniform` + `color` | GPU (`fill_uniform`) | a solid fill is one store per texel |
//! | `uniform` + `value` | CPU (`ScalarConst`) | no raster to compute — a constant |
//! | `image_asset` | CPU (`AssetLoad`) | filesystem PNG decode, then uploaded as an input texture |
//! | `passthrough` | alias (no dispatch) | forwards the upstream handle/bytes |
//! | `noise_perlin`, `noise_value`, `noise_worley` | GPU | pure uv→gray functions over the uploaded perm table |
//! | `gradient`, `checkerboard`, `dots`, `brick_pattern` | GPU | pure uv functions of their params |
//! | `blur` | GPU iff `radius <= 256`, else CPU (`LargeKernel`) | separable box; each pass taps `2r+1` texels serially per output |
//! | `sharpen` | GPU iff `radius <= 256`, else CPU (`LargeKernel`) | two blur passes + one unsharp stage |
//! | `levels`, `curves`, `invert` | GPU | single-input per-pixel point ops |
//! | `mix`, `color_correct`, `hsv_adjust` | GPU | one-to-two-input point ops |
//! | `triplanar_blend` | GPU | three-input weighted blend |
//! | `edge_detect` | GPU | dilated Sobel kernel with edge clamping |
//! | `direction_warp` | GPU | bilinear resample with edge clamping |
//! | `flood_fill`, `histogram_match` | CPU (`GlobalPass`) | connected-component labeling and CDF transfer need whole-image global passes/reductions — no single per-pixel dispatch expresses them |
//! | `synth` | CPU (`GlobalPass`) | multiresolution example-based synthesis is an iterative whole-image search (coherence propagation across raster order) — no per-pixel GPU dispatch expresses it either |
//! | `mesh_map_generator` | CPU (`External`) | bridge-provided bytes (bake outputs); cloned/uploaded, never computed |
//! | `anchor` | CPU (`External`) | bridge-provided bytes (the bound layer's composited channel); clone, never computed |
//! | unregistered defs, incl. WASM guests | CPU (`PluginGuest`) | arbitrary guest/host code — dispatched through the registry like the CPU engine |
//!
//! # Tiles
//!
//! A tile is a UDIM tile id (`u16`, the `umber_core`-side model in
//! `umber-core/src/tiles.rs` — umber-gpu stays dependency-minimal and
//! takes plain ids, not that type) carrying its own resolution
//! ([`TileSpec`]): one dispatch per (node, tile) pair, the dirty-node set
//! crossed with the dirty tiles ([`plan_dispatches`]). Multi-tile callers
//! loop [`GraphGpuEvaluator::evaluate`] per tile; each tile evaluates at
//! its own resolution with the same externals (v1 — per-tile externals
//! are wave-6).
//!
//! # Judgment calls (for the static verifier)
//!
//! - One shared bind-group layout for all 20 entry points, unused image
//!   inputs bound to a 1x1 fallback (the `MESH_SHADER` IBL-set
//!   convention) — so the pipeline cache is keyed by entry-point name,
//!   never rebuilt per graph shape.
//! - The blur `radius <= 256` budget is routing, not validation: an
//!   over-budget blur evaluates on the CPU with identical bytes, never an
//!   error. 256 taps × 2 passes is the largest serial tap loop this slice
//!   defends; the CPU kernel is O(n) for any radius, the GPU one is O(r)
//!   per texel.
//! - CPU-fallback nodes inside a GPU eval run through the same registry
//!   (same validation, same errors, stamped with the node id like the CPU
//!   dispatcher). Boundary crossings upload (CPU→GPU, via `queue.write_*`
//!   before the consuming dispatch's submit) or read back (GPU→CPU, via a
//!   submit boundary + blocking map, the `PaintTarget::read_back_rgba8`
//!   pattern). Correct first, resident-cache fast later (wave-6).
//! - The CPU-side [`umber_graph::EvalCache`] stays byte-based: GPU outputs
//!   are read back into it, so a GPU eval and a CPU eval leave identical
//!   caches and `evaluate` returns the same full output map as
//!   `eval_graph_cached`.
//! - `GraphGpuEvaluator::new` is infallible and builds no pipelines:
//!   pipelines compile lazily on first dispatch and are reused across
//!   evals (pinned by test). No adapter-specific device feature is
//!   required — all stores are `write`-only (core WebGPU).

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::mem::size_of;
use std::num::NonZeroU64;

use wgpu::util::DeviceExt as _;

use umber_graph::{
    EvalCache, EvalContext, EvalError, Graph, GraphError, ImageBuffer, ImageError, Node,
    NodeOutput, NodeRegistry, ParamValue,
};

use crate::graph_shaders::{
    ENTRY_BLUR_H, ENTRY_BLUR_V, ENTRY_BRICK, ENTRY_CHECKERBOARD, ENTRY_COLOR_CORRECT, ENTRY_CURVES,
    ENTRY_DOTS, ENTRY_EDGE_DETECT, ENTRY_FILL_UNIFORM, ENTRY_GRADIENT, ENTRY_HSV, ENTRY_INVERT,
    ENTRY_LEVELS, ENTRY_MIX, ENTRY_NOISE_PERLIN, ENTRY_NOISE_VALUE, ENTRY_NOISE_WORLEY,
    ENTRY_SHARPEN, ENTRY_TRIPLANAR, ENTRY_WARP,
};

/// Which evaluation backend a caller wants (the panel's selectable path —
/// see `umber_app::graph_panel`; CPU stays the reference in all cases).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EvalBackend {
    /// GPU when an evaluator is attached, else CPU (the default: current
    /// behavior until the app wires a device in).
    #[default]
    Auto,
    /// Always the CPU cached-eval.
    Cpu,
    /// Always the GPU evaluator (an error when none is attached — never a
    /// silent CPU run, so a caller asking for GPU knows what it got).
    Gpu,
}

/// One dirty tile: its UDIM id plus the resolution this tile evaluates
/// at (each tile keeps its own w/h, mirroring `TileChannels`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileSpec {
    /// UDIM tile id (1001..=1100 in the core model).
    pub id: u16,
    /// This tile's raster width.
    pub width: u32,
    /// This tile's raster height.
    pub height: u32,
}

impl TileSpec {
    /// Builds a tile spec from its id and resolution.
    pub fn new(id: u16, width: u32, height: u32) -> Self {
        Self { id, width, height }
    }

    /// The default single tile (1001) at `width`x`height` — what the
    /// panel passes until the multi-tile (UDIM-set-driven) loop lands.
    pub fn tile_1001(width: u32, height: u32) -> Self {
        Self::new(umber_mesh::FIRST_TILE, width, height)
    }
}

/// A GPU entry-point group: one node type's compute work. Multi-pass node
/// types ([`GpuOp::Blur`], [`GpuOp::Sharpen`]) expand to several entry
/// points (see [`GpuOp::entry_points`]); everything else is one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuOp {
    /// `uniform` with a `color` param.
    FillUniform,
    /// `noise_perlin`.
    NoisePerlin,
    /// `noise_value`.
    NoiseValue,
    /// `noise_worley`.
    NoiseWorley,
    /// `gradient`.
    Gradient,
    /// `checkerboard`.
    Checkerboard,
    /// `dots`.
    Dots,
    /// `brick_pattern`.
    Brick,
    /// `invert`.
    Invert,
    /// `levels`.
    Levels,
    /// `curves`.
    Curves,
    /// `mix`.
    Mix,
    /// `color_correct`.
    ColorCorrect,
    /// `hsv_adjust`.
    HsvAdjust,
    /// `triplanar_blend`.
    Triplanar,
    /// `edge_detect`.
    EdgeDetect,
    /// `direction_warp`.
    Warp,
    /// `blur` (horizontal + vertical separable passes).
    Blur,
    /// `sharpen` (blur passes + the unsharp stage).
    Sharpen,
}

impl GpuOp {
    /// The shader entry point(s) this op dispatches, in order. The
    /// strings are exactly [`crate::graph_shaders::GRAPH_ENTRY_POINTS`]
    /// members (pinned by test on both sides).
    pub fn entry_points(&self) -> &'static [&'static str] {
        match self {
            Self::FillUniform => &[ENTRY_FILL_UNIFORM],
            Self::NoisePerlin => &[ENTRY_NOISE_PERLIN],
            Self::NoiseValue => &[ENTRY_NOISE_VALUE],
            Self::NoiseWorley => &[ENTRY_NOISE_WORLEY],
            Self::Gradient => &[ENTRY_GRADIENT],
            Self::Checkerboard => &[ENTRY_CHECKERBOARD],
            Self::Dots => &[ENTRY_DOTS],
            Self::Brick => &[ENTRY_BRICK],
            Self::Invert => &[ENTRY_INVERT],
            Self::Levels => &[ENTRY_LEVELS],
            Self::Curves => &[ENTRY_CURVES],
            Self::Mix => &[ENTRY_MIX],
            Self::ColorCorrect => &[ENTRY_COLOR_CORRECT],
            Self::HsvAdjust => &[ENTRY_HSV],
            Self::Triplanar => &[ENTRY_TRIPLANAR],
            Self::EdgeDetect => &[ENTRY_EDGE_DETECT],
            Self::Warp => &[ENTRY_WARP],
            Self::Blur => &[ENTRY_BLUR_H, ENTRY_BLUR_V],
            Self::Sharpen => &[ENTRY_BLUR_H, ENTRY_BLUR_V, ENTRY_SHARPEN],
        }
    }
}

/// Why a node evaluates on the CPU inside a GPU run (see the module-doc
/// split table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuPath {
    /// `image_asset`: filesystem decode, then uploaded as a texture.
    AssetLoad,
    /// `flood_fill`, `histogram_match`, `synth`: whole-image global
    /// passes.
    GlobalPass,
    /// `mesh_map_generator`: bridge-provided bytes, cloned/uploaded.
    External,
    /// `uniform` value: a scalar constant, no raster.
    ScalarConst,
    /// `blur`/`sharpen` past the v1 GPU tap budget (`radius > 256`).
    LargeKernel,
    /// Unregistered defs, including WASM guests: arbitrary CPU code.
    PluginGuest,
}

/// Where one node evaluates in a GPU run. Total over `node_def`s (plus
/// params for the param-dependent arms): no node is ever unrouted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuRoute {
    /// A WGSL compute dispatch ([`GpuOp`]).
    Gpu(GpuOp),
    /// Evaluated through the registry on the CPU (same errors as the CPU
    /// engine), with texture upload/readback at the boundary.
    Cpu(CpuPath),
    /// `passthrough`: forwards the upstream handle/bytes, no dispatch.
    Alias,
}

/// The v1 GPU tap budget: `blur`/`sharpen` past this radius evaluate on
/// the CPU (see the module docs — routing, not validation).
pub const MAX_GPU_BLUR_RADIUS: i32 = 256;

impl GpuRoute {
    /// Routes one node. Param-dependent arms (`uniform`'s color/value
    /// split, the blur radius budget) read only params, never inputs —
    /// so routing is a pure pre-pass over the graph (see
    /// [`plan_dispatches`]).
    pub fn route(node_def: &str, params: &[(String, ParamValue)]) -> Self {
        match node_def {
            "uniform" => {
                if params.iter().any(|(n, _)| n == "color") {
                    Self::Gpu(GpuOp::FillUniform)
                } else {
                    Self::Cpu(CpuPath::ScalarConst)
                }
            }
            "image_asset" => Self::Cpu(CpuPath::AssetLoad),
            "passthrough" => Self::Alias,
            "noise_perlin" => Self::Gpu(GpuOp::NoisePerlin),
            "noise_value" => Self::Gpu(GpuOp::NoiseValue),
            "noise_worley" => Self::Gpu(GpuOp::NoiseWorley),
            "gradient" => Self::Gpu(GpuOp::Gradient),
            "checkerboard" => Self::Gpu(GpuOp::Checkerboard),
            "dots" => Self::Gpu(GpuOp::Dots),
            "brick_pattern" => Self::Gpu(GpuOp::Brick),
            "blur" => {
                if blur_radius_param(params).is_some_and(|r| r > MAX_GPU_BLUR_RADIUS) {
                    Self::Cpu(CpuPath::LargeKernel)
                } else {
                    Self::Gpu(GpuOp::Blur)
                }
            }
            "sharpen" => {
                if blur_radius_param(params).is_some_and(|r| r > MAX_GPU_BLUR_RADIUS) {
                    Self::Cpu(CpuPath::LargeKernel)
                } else {
                    Self::Gpu(GpuOp::Sharpen)
                }
            }
            "levels" => Self::Gpu(GpuOp::Levels),
            "curves" => Self::Gpu(GpuOp::Curves),
            "invert" => Self::Gpu(GpuOp::Invert),
            "mix" => Self::Gpu(GpuOp::Mix),
            "color_correct" => Self::Gpu(GpuOp::ColorCorrect),
            "hsv_adjust" => Self::Gpu(GpuOp::HsvAdjust),
            "flood_fill" | "histogram_match" | "synth" => Self::Cpu(CpuPath::GlobalPass),
            "edge_detect" => Self::Gpu(GpuOp::EdgeDetect),
            "direction_warp" => Self::Gpu(GpuOp::Warp),
            "triplanar_blend" => Self::Gpu(GpuOp::Triplanar),
            "mesh_map_generator" => Self::Cpu(CpuPath::External),
            // Anchors are bridge-provided bytes like mesh maps (the layer's
            // composited channel, uploaded under `anchor:{id}:{channel}`).
            "anchor" => Self::Cpu(CpuPath::External),
            _ => Self::Cpu(CpuPath::PluginGuest),
        }
    }
}

/// Reads the blur/sharpen `radius: Int` param for routing (`None` when
/// absent or mistyped — a mistyped radius routes GPU and fails honestly
/// at pack time with `BadParam`, exactly like the CPU node).
fn blur_radius_param(params: &[(String, ParamValue)]) -> Option<i32> {
    match params.iter().find(|(n, _)| n == "radius") {
        None => Some(1),
        Some((_, ParamValue::Int(r))) => Some(*r),
        Some(_) => None,
    }
}

/// One planned (node, tile) dispatch: the dirty-set × dirty-tiles cross
/// product in topo order (see [`plan_dispatches`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedDispatch {
    /// The node to (re-)evaluate.
    pub node: u64,
    /// The tile to evaluate it for.
    pub tile: u16,
    /// How it evaluates (routed from the node's def + params).
    pub route: GpuRoute,
}

/// Everything that can go wrong in a GPU evaluation. CPU-path failures
/// reuse [`EvalError`]/[`GraphError`] verbatim so panel status lines read
/// identically whichever backend ran.
#[derive(Debug, thiserror::Error)]
pub enum GraphGpuError {
    /// Topology rejection (dangling refs, duplicates, cycles) — validated
    /// before any dispatch, like the CPU engine.
    #[error(transparent)]
    Graph(#[from] GraphError),
    /// Param/size/CPU-branch failures — the same type the CPU eval
    /// returns (node ids stamped by the dispatcher, same as `eval.rs`).
    #[error(transparent)]
    Eval(#[from] EvalError),
    /// `evaluate` was called with an empty tile list (nothing to target).
    #[error("no tiles to evaluate (empty tile list)")]
    NoTiles,
    /// A GPU texture readback failed (device poll, map callback, range).
    #[error("graph readback failed: {0}")]
    Readback(String),
}

/// A lazy pipeline cache keyed by shader entry-point name: pipelines
/// compile on first dispatch and are reused across evals for the
/// evaluator's lifetime. Generic over the pipeline object so the
/// reuse logic itself is unit-testable without a device (see tests).
#[derive(Debug)]
pub struct PipelineCache<P> {
    entries: HashMap<&'static str, P>,
}

impl<P> Default for PipelineCache<P> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<P> PipelineCache<P> {
    /// An empty cache (first dispatch of each entry compiles it).
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `entry` already has a compiled pipeline.
    pub fn contains(&self, entry: &'static str) -> bool {
        self.entries.contains_key(entry)
    }

    /// The cached pipeline for `entry`, if compiled.
    pub fn get(&self, entry: &'static str) -> Option<&P> {
        self.entries.get(entry)
    }

    /// Records a freshly compiled pipeline.
    pub fn insert(&mut self, entry: &'static str, pipeline: P) {
        self.entries.insert(entry, pipeline);
    }

    /// The cached pipeline, compiling it with `make` on first use. The
    /// factory runs at most once per entry (pinned by test).
    pub fn get_or_insert_with(&mut self, entry: &'static str, make: impl FnOnce() -> P) -> &P {
        self.entries.entry(entry).or_insert_with(make)
    }

    /// Count of compiled pipelines.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no pipeline has been compiled yet.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The WGSL `GraphParams` uniform (four `vec4<f32>` slots, 64 bytes —
/// mirrors `GRAPH_EVAL_SHADER`'s struct field-for-field; the per-op
/// packing is documented on that constant).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GraphParams {
    /// First slot (the op-specific packing's head — see per-op docs).
    pub a: [f32; 4],
    /// Second slot.
    pub b: [f32; 4],
    /// Third slot (currently unused by every entry, zeroed).
    pub c: [f32; 4],
    /// Fourth slot (currently unused by every entry, zeroed).
    pub d: [f32; 4],
}

const _: () = assert!(
    size_of::<GraphParams>() == 64,
    "GraphParams must match GRAPH_EVAL_SHADER's GraphParams layout"
);

impl GraphParams {
    /// Zeroed params (unused slots stay zero — entries only read the
    /// slots their packing documents).
    fn of(a: [f32; 4], b: [f32; 4]) -> Self {
        Self {
            a,
            b,
            c: [0.0; 4],
            d: [0.0; 4],
        }
    }
}

// --- Param validation (mirrors the CPU impls' rules so the GPU path
// --- rejects the same graphs with the same BadParam payloads) ---

fn find_param<'a>(params: &'a [(String, ParamValue)], name: &str) -> Option<&'a ParamValue> {
    params.iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

fn bad_param(name: &str) -> EvalError {
    EvalError::BadParam {
        node: 0,
        param: name.into(),
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
        Some(_) => return Err(bad_param(name)),
    };
    if !v.is_finite() {
        return Err(bad_param(name));
    }
    Ok(v)
}

fn int_param(params: &[(String, ParamValue)], name: &str, default: i32) -> Result<i32, EvalError> {
    match find_param(params, name) {
        None => Ok(default),
        Some(ParamValue::Int(v)) => Ok(*v),
        Some(_) => Err(bad_param(name)),
    }
}

fn vec2_param(
    params: &[(String, ParamValue)],
    name: &str,
    default: [f32; 2],
) -> Result<[f32; 2], EvalError> {
    let v = match find_param(params, name) {
        None => default,
        Some(ParamValue::Vec2(v)) => *v,
        Some(_) => return Err(bad_param(name)),
    };
    if !v[0].is_finite() || !v[1].is_finite() {
        return Err(bad_param(name));
    }
    Ok(v)
}

fn vec3_param(
    params: &[(String, ParamValue)],
    name: &str,
    default: [f32; 3],
) -> Result<[f32; 3], EvalError> {
    let v = match find_param(params, name) {
        None => default,
        Some(ParamValue::Vec3(v)) => *v,
        Some(_) => return Err(bad_param(name)),
    };
    if !v.iter().all(|c| c.is_finite()) {
        return Err(bad_param(name));
    }
    Ok(v)
}

/// Enforces the v1 raster ceiling on an output extent (the same ceiling
/// `ImageBuffer` enforces — a hostile param must not allocate the GPU).
fn check_dims(width: u32, height: u32) -> Result<(), EvalError> {
    if width == 0 || height == 0 {
        return Err(EvalError::Image(ImageError::EmptyDimensions {
            width,
            height,
        }));
    }
    if width > 8192 || height > 8192 || width as u64 * height as u64 > (1u64 << 26) {
        return Err(EvalError::Image(ImageError::DimensionsTooLarge {
            width,
            height,
        }));
    }
    Ok(())
}

/// A packed GPU node: validated params + output extent + (for noise) the
/// permutation-table seed to build the storage buffer from.
#[derive(Debug)]
struct PackedNode {
    params: GraphParams,
    perm_seed: Option<i32>,
    width: u32,
    height: u32,
}

/// Validates `params` for `op` (same rules as the CPU impl) and packs the
/// uniform. `input` is the resolved input extent for filter nodes
/// (`None` only when the caller has not resolved one — a `BadParam`, kept
/// honest rather than panicking). `tile` supplies the generator extent.
fn pack_node(
    op: GpuOp,
    params: &[(String, ParamValue)],
    input: Option<(u32, u32)>,
    tile: &TileSpec,
) -> Result<PackedNode, EvalError> {
    let gen = || -> Result<(u32, u32), EvalError> {
        check_dims(tile.width, tile.height)?;
        Ok((tile.width, tile.height))
    };
    let filter = || -> Result<(u32, u32), EvalError> {
        match input {
            Some((w, h)) => {
                check_dims(w, h)?;
                Ok((w, h))
            }
            None => Err(bad_param("expected exactly 1 input, got 0")),
        }
    };
    let (p, seed, (w, h)) = match op {
        GpuOp::FillUniform => {
            let rgb = match find_param(params, "color") {
                Some(ParamValue::Color(c)) => *c,
                _ => return Err(bad_param("color")),
            };
            // The `resolution` override mirrors the CPU uniform node
            // (param or tile extent, same range checks).
            let (w, h) = match find_param(params, "resolution") {
                None => {
                    check_dims(tile.width, tile.height)?;
                    (tile.width, tile.height)
                }
                Some(ParamValue::Vec2([w, h])) => {
                    if !w.is_finite() || !h.is_finite() {
                        return Err(bad_param("resolution"));
                    }
                    let (w, h) = (w.round(), h.round());
                    if w < 1.0 || h < 1.0 || w > 8192.0 || h > 8192.0 {
                        return Err(bad_param("resolution"));
                    }
                    (w as u32, h as u32)
                }
                Some(_) => return Err(bad_param("resolution")),
            };
            (
                GraphParams::of([rgb[0], rgb[1], rgb[2], 0.0], [0.0; 4]),
                None,
                (w, h),
            )
        }
        GpuOp::NoisePerlin | GpuOp::NoiseValue | GpuOp::NoiseWorley => {
            let (w, h) = gen()?;
            let scale = float_param(params, "scale", 8.0)?;
            let offset = vec2_param(params, "offset", [0.0, 0.0])?;
            let seed = int_param(params, "seed", 0)?;
            (
                GraphParams::of([scale, offset[0], offset[1], 0.0], [0.0; 4]),
                Some(seed),
                (w, h),
            )
        }
        GpuOp::Gradient => {
            let (w, h) = gen()?;
            let kind = int_param(params, "type", 0)?;
            if !(0..=2).contains(&kind) {
                return Err(bad_param("type"));
            }
            let repeat = int_param(params, "repeat", 1)?;
            if repeat < 1 {
                return Err(bad_param("repeat"));
            }
            let center = vec2_param(params, "center", [0.5, 0.5])?;
            let angle = float_param(params, "angle", 0.0)?;
            (
                GraphParams::of(
                    [kind as f32, repeat as f32, center[0], center[1]],
                    [angle, 0.0, 0.0, 0.0],
                ),
                None,
                (w, h),
            )
        }
        GpuOp::Checkerboard => {
            let (w, h) = gen()?;
            let scale = float_param(params, "scale", 8.0)?;
            if scale <= 0.0 {
                return Err(bad_param("scale"));
            }
            (
                GraphParams::of([scale, 0.0, 0.0, 0.0], [0.0; 4]),
                None,
                (w, h),
            )
        }
        GpuOp::Dots => {
            let (w, h) = gen()?;
            let scale = float_param(params, "scale", 8.0)?;
            if scale <= 0.0 {
                return Err(bad_param("scale"));
            }
            let radius = float_param(params, "radius", 0.25)?;
            if radius < 0.0 {
                return Err(bad_param("radius"));
            }
            (
                GraphParams::of([scale, radius, 0.0, 0.0], [0.0; 4]),
                None,
                (w, h),
            )
        }
        GpuOp::Brick => {
            let (w, h) = gen()?;
            let (sx, sy) = match find_param(params, "scale") {
                None => (8.0, 8.0),
                Some(ParamValue::Vec2([a, b])) => (*a, *b),
                Some(ParamValue::Float(a)) => (*a, *a),
                Some(_) => return Err(bad_param("scale")),
            };
            if !sx.is_finite() || !sy.is_finite() || sx <= 0.0 || sy <= 0.0 {
                return Err(bad_param("scale"));
            }
            let mortar = float_param(params, "mortar", 0.1)?;
            if !(0.0..1.0).contains(&mortar) {
                return Err(bad_param("mortar"));
            }
            (
                GraphParams::of([sx, sy, mortar, 0.0], [0.0; 4]),
                None,
                (w, h),
            )
        }
        GpuOp::Invert => {
            let (w, h) = filter()?;
            (GraphParams::of([0.0; 4], [0.0; 4]), None, (w, h))
        }
        GpuOp::Levels => {
            let (w, h) = filter()?;
            let in_low = float_param(params, "in_low", 0.0)?;
            let in_high = float_param(params, "in_high", 1.0)?;
            let gamma = float_param(params, "gamma", 1.0)?;
            let out_low = float_param(params, "out_low", 0.0)?;
            let out_high = float_param(params, "out_high", 1.0)?;
            if gamma < 0.0 {
                return Err(bad_param("gamma"));
            }
            if in_high == in_low {
                return Err(bad_param("in_low == in_high (degenerate range)"));
            }
            (
                GraphParams::of([in_low, in_high, gamma, 0.0], [out_low, out_high, 0.0, 0.0]),
                None,
                (w, h),
            )
        }
        GpuOp::Curves => {
            let (w, h) = filter()?;
            let pt = |name: &str| -> Result<[f32; 2], EvalError> {
                match find_param(params, name) {
                    Some(ParamValue::Vec2(v)) => Ok(*v),
                    _ => Err(bad_param(name)),
                }
            };
            let (p0, p1, p2, p3) = (
                pt("curve_p0")?,
                pt("curve_p1")?,
                pt("curve_p2")?,
                pt("curve_p3")?,
            );
            for p in [p0, p1, p2, p3] {
                if !p[0].is_finite() || !p[1].is_finite() {
                    return Err(bad_param("curve_p* (non-finite coordinate)"));
                }
            }
            if !(p0[0] < p1[0] && p1[0] < p2[0] && p2[0] < p3[0]) {
                return Err(bad_param("curve_p* (x must be strictly increasing)"));
            }
            (
                GraphParams::of([p0[0], p0[1], p1[0], p1[1]], [p2[0], p2[1], p3[0], p3[1]]),
                None,
                (w, h),
            )
        }
        GpuOp::Mix => {
            let (w, h) = filter()?;
            let factor = float_param(params, "factor", 0.5)?;
            if !(0.0..=1.0).contains(&factor) {
                return Err(bad_param("factor"));
            }
            (
                GraphParams::of([factor, 0.0, 0.0, 0.0], [0.0; 4]),
                None,
                (w, h),
            )
        }
        GpuOp::ColorCorrect => {
            let (w, h) = filter()?;
            let hue = float_param(params, "hue", 0.0)?;
            let saturation = float_param(params, "saturation", 1.0)?;
            let brightness = float_param(params, "brightness", 0.0)?;
            let contrast = float_param(params, "contrast", 1.0)?;
            (
                GraphParams::of([hue, saturation, brightness, contrast], [0.0; 4]),
                None,
                (w, h),
            )
        }
        GpuOp::HsvAdjust => {
            let (w, h) = filter()?;
            let hue = float_param(params, "hue", 0.0)?;
            let saturation = float_param(params, "saturation", 1.0)?;
            let v_gain = float_param(params, "v_gain", 1.0)?;
            (
                GraphParams::of([hue, saturation, v_gain, 0.0], [0.0; 4]),
                None,
                (w, h),
            )
        }
        GpuOp::Triplanar => {
            let (w, h) = filter()?;
            let wv = vec3_param(params, "weights", [1.0, 1.0, 1.0])?;
            let sum = wv[0] + wv[1] + wv[2];
            if sum == 0.0 {
                return Err(bad_param("weights sum to zero"));
            }
            (
                GraphParams::of([wv[0] / sum, wv[1] / sum, wv[2] / sum, 0.0], [0.0; 4]),
                None,
                (w, h),
            )
        }
        GpuOp::EdgeDetect => {
            let (w, h) = filter()?;
            let radius = int_param(params, "radius", 1)?.clamp(1, 3);
            (
                GraphParams::of([radius as f32, 0.0, 0.0, 0.0], [0.0; 4]),
                None,
                (w, h),
            )
        }
        GpuOp::Warp => {
            let (w, h) = filter()?;
            let vector = vec2_param(params, "vector", [0.015, 0.0])?;
            let strength = float_param(params, "strength", 0.05)?;
            // `use_map` is OR'd in by the dispatcher (fourth slot), which
            // knows whether the vector input is wired.
            (
                GraphParams::of([vector[0], vector[1], strength, 0.0], [0.0; 4]),
                None,
                (w, h),
            )
        }
        GpuOp::Blur | GpuOp::Sharpen => {
            let (w, h) = filter()?;
            let radius = int_param(params, "radius", 1)?.max(0);
            // Routing already sent `radius > 256` to the CPU; the clamp
            // below is a belt-and-braces (a direct `pack_node` caller must
            // not hang the GPU with a hostile tap count).
            let radius = radius.min(MAX_GPU_BLUR_RADIUS);
            if op == GpuOp::Blur {
                (
                    GraphParams::of([radius as f32, 0.0, 0.0, 0.0], [0.0; 4]),
                    None,
                    (w, h),
                )
            } else {
                let amount = float_param(params, "amount", 0.5)?;
                (
                    GraphParams::of([amount, radius as f32, 0.0, 0.0], [0.0; 4]),
                    None,
                    (w, h),
                )
            }
        }
    };
    Ok(PackedNode {
        params: p,
        perm_seed: seed,
        width: w,
        height: h,
    })
}

// --- Device-free planning (dirty set -> dispatch set) ---

/// Deterministic Kahn order (ascending ids first) — the same convention
/// as `umber_graph::eval`'s private sweep, restated here because that
/// function is crate-private. Precondition: the graph validated.
fn topo_order_eval(graph: &Graph) -> Result<Vec<u64>, GraphError> {
    let mut indegree: HashMap<u64, usize> = graph.nodes.iter().map(|n| (n.id, 0)).collect();
    let mut outs: HashMap<u64, Vec<u64>> = HashMap::new();
    for e in &graph.edges {
        *indegree.get_mut(&e.to).expect("validated") += 1;
        outs.entry(e.from).or_default().push(e.to);
    }
    let mut queue: Vec<u64> = indegree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&id, _)| id)
        .collect();
    queue.sort_unstable();
    let mut order = Vec::with_capacity(graph.nodes.len());
    while let Some(&id) = queue.first() {
        queue.remove(0);
        order.push(id);
        if let Some(next) = outs.get(&id) {
            for &to in next {
                let d = indegree.get_mut(&to).expect("validated");
                *d -= 1;
                if *d == 0 {
                    queue.push(to);
                    queue.sort_unstable();
                }
            }
        }
    }
    if order.len() != graph.nodes.len() {
        let cyclic: Vec<u64> = indegree
            .iter()
            .filter(|(_, &d)| d > 0)
            .map(|(&id, _)| id)
            .collect();
        return Err(GraphError::Cycle(cyclic));
    }
    Ok(order)
}

fn node_by_id(graph: &Graph, id: u64) -> &Node {
    graph
        .nodes
        .iter()
        .find(|n| n.id == id)
        .expect("topo order only contains graph nodes")
}

/// Computes the run set for a dirty re-evaluation: `dirty` seeds it,
/// edges propagate it forward, and cache misses count as dirty (a node
/// with no cached output always runs, and its downstream re-runs) — the
/// exact `eval_graph_cached` rule, factored out so the GPU path and the
/// dispatch planner share it without a device.
///
/// Returns the topo order plus the run set.
///
/// # Errors
///
/// Propagates [`Graph::validate`] failures.
pub fn compute_run_set(
    graph: &Graph,
    dirty: &HashSet<u64>,
    cached: &HashSet<u64>,
) -> Result<(Vec<u64>, HashSet<u64>), GraphError> {
    graph.validate()?;
    let order = topo_order_eval(graph)?;
    let mut downstream: HashMap<u64, Vec<u64>> = HashMap::new();
    for e in &graph.edges {
        downstream.entry(e.from).or_default().push(e.to);
    }
    let mut run: HashSet<u64> = dirty.clone();
    for id in &order {
        if run.contains(id) || !cached.contains(id) {
            run.insert(*id);
            if let Some(next) = downstream.get(id) {
                for &to in next {
                    run.insert(to);
                }
            }
        }
    }
    Ok((order, run))
}

/// Plans the (node, tile) dispatch set for a dirty re-evaluation: the
/// run set from [`compute_run_set`] crossed with `tiles`, in topo order
/// per tile. Pure CPU logic — the unit-testable half of the per-dirty-
/// tile dispatch loop (the device half is
/// [`GraphGpuEvaluator::evaluate`]).
///
/// # Errors
///
/// Propagates [`Graph::validate`] failures.
pub fn plan_dispatches(
    graph: &Graph,
    dirty: &HashSet<u64>,
    cached: &HashSet<u64>,
    tiles: &[TileSpec],
) -> Result<Vec<PlannedDispatch>, GraphGpuError> {
    let (order, run) = compute_run_set(graph, dirty, cached)?;
    let mut plan = Vec::new();
    for tile in tiles {
        for id in &order {
            if run.contains(id) {
                let node = node_by_id(graph, *id);
                plan.push(PlannedDispatch {
                    node: *id,
                    tile: tile.id,
                    route: GpuRoute::route(&node.node_def, &node.params),
                });
            }
        }
    }
    Ok(plan)
}

// --- The evaluator ---

/// One intermediate GPU image: a node output (or an uploaded CPU image)
/// living in a `Rgba8Unorm` texture.
#[derive(Debug, Clone)]
struct GpuImage {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

fn make_texture(device: &wgpu::Device, width: u32, height: u32) -> GpuImage {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("umber_graph_eval_intermediate"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    GpuImage {
        texture,
        view,
        width,
        height,
    }
}

/// A resolved node input: either a resident GPU texture or CPU bytes
/// awaiting upload.
#[derive(Debug, Clone)]
enum Resolved {
    Gpu(GpuImage),
    Cpu(NodeOutput),
}

fn upload_texture(device: &wgpu::Device, queue: &wgpu::Queue, buf: &ImageBuffer) -> GpuImage {
    // `queue.write_texture` (not an encoder copy): the upload lands at
    // submit time, ahead of every dispatch recorded later on the
    // evaluator's encoder — no ordering hazard. (The paint pass avoids
    // queue writes for per-call dab buffers shared across passes on ONE
    // encoder; uploads here always precede their consuming dispatch.)
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("umber_graph_eval_upload"),
        size: wgpu::Extent3d {
            width: buf.width,
            height: buf.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &buf.data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(buf.width * 4),
            rows_per_image: Some(buf.height),
        },
        wgpu::Extent3d {
            width: buf.width,
            height: buf.height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    GpuImage {
        texture,
        view,
        width: buf.width,
        height: buf.height,
    }
}

/// Reads one intermediate back as a CPU [`ImageBuffer`] (the padded-copy
/// + blocking-map pattern from `PaintTarget::read_back_rgba8`).
fn readback_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &GpuImage,
) -> Result<ImageBuffer, GraphGpuError> {
    let readback_err = |what: String| GraphGpuError::Readback(what);
    let (width, height) = (img.width, img.height);
    let unpadded_row = width * 4;
    let padding = (256 - (unpadded_row % 256)) % 256;
    let padded_row = unpadded_row + padding;
    let size = padded_row as u64 * height as u64;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("umber_graph_eval_readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("umber_graph_eval_readback_encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &img.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .map_err(|e| readback_err(format!("device poll: {e}")))?;
    receiver
        .recv()
        .map_err(|_| readback_err("map callback channel closed".into()))?
        .map_err(|e| readback_err(format!("buffer map: {e}")))?;

    let data = slice
        .get_mapped_range()
        .map_err(|e| readback_err(format!("mapped range: {e}")))?;
    let bytes: &[u8] = &data;
    let mut out = Vec::with_capacity(unpadded_row as usize * height as usize);
    for row in 0..height as usize {
        let start = row * padded_row as usize;
        out.extend_from_slice(&bytes[start..start + unpadded_row as usize]);
    }
    drop(data);
    buffer.unmap();
    ImageBuffer::new(width, height, out).map_err(|e| readback_err(e.to_string()))
}

/// Materializes a resolved input as an owned GPU texture handle:
/// resident handles pass through, CPU/cached images upload.
fn mat_tex(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    input: &Resolved,
    node: u64,
) -> Result<GpuImage, EvalError> {
    match input {
        Resolved::Gpu(t) => Ok(t.clone()),
        Resolved::Cpu(NodeOutput::Image(buf)) => Ok(upload_texture(device, queue, buf)),
        Resolved::Cpu(other) => Err(EvalError::TypeMismatch {
            node,
            expected: "Image".into(),
            got: other.kind().into(),
        }),
    }
}

/// Looks up one upstream output across the fresh GPU/CPU maps with a
/// cache fallback (the `resolve_inputs_cached` rule: fresh wins, cached
/// fills gaps).
fn lookup_upstream<'a>(
    id: u64,
    gpu: &'a HashMap<u64, GpuImage>,
    cpu: &'a HashMap<u64, NodeOutput>,
    cache: &'a EvalCache,
) -> Option<Resolved> {
    if let Some(t) = gpu.get(&id) {
        return Some(Resolved::Gpu(t.clone()));
    }
    if let Some(v) = cpu.get(&id) {
        return Some(Resolved::Cpu(v.clone()));
    }
    cache.get(id).cloned().map(Resolved::Cpu)
}

/// Resolves a node's inputs (sorted by name for determinism): `NodeRef`
/// params first in param order, then `Anchor` params against `external`
/// (as [`Resolved::Cpu`] — bridge bytes, uploaded at the boundary when a
/// GPU node consumes them), then edges in graph order overlaying
/// same-named entries — the `eval.rs` rule, restated over [`Resolved`].
fn resolve_inputs_gpu(
    graph: &Graph,
    node: &Node,
    gpu: &HashMap<u64, GpuImage>,
    cpu: &HashMap<u64, NodeOutput>,
    cache: &EvalCache,
    external: &HashMap<String, NodeOutput>,
) -> Result<Vec<(String, Resolved)>, EvalError> {
    let mut map: BTreeMap<String, Resolved> = BTreeMap::new();
    for (name, pv) in &node.params {
        if let ParamValue::NodeRef(up) = pv {
            let v =
                lookup_upstream(*up, gpu, cpu, cache).ok_or_else(|| EvalError::MissingInput {
                    node: node.id,
                    input: name.clone(),
                })?;
            map.insert(name.clone(), v);
        }
        // Anchors resolve through the same externals key as the CPU
        // engine (`anchor::{id}:{channel}`); a bound anchor with no
        // external is the typed Anchor error (deleted layer), on both
        // backends alike.
        if let ParamValue::Anchor(anchor) = pv {
            let v = external
                .get(&anchor.key())
                .cloned()
                .ok_or_else(|| EvalError::Anchor {
                    node: node.id,
                    layer: anchor.layer_id,
                    channel: anchor.channel.clone(),
                    reason: "layer deleted or never composited".into(),
                })?;
            map.insert(name.clone(), Resolved::Cpu(v));
        }
    }
    for e in &graph.edges {
        if e.to == node.id {
            let v = lookup_upstream(e.from, gpu, cpu, cache).ok_or_else(|| {
                EvalError::MissingInput {
                    node: node.id,
                    input: e.input.clone(),
                }
            })?;
            map.insert(e.input.clone(), v);
        }
    }
    Ok(map.into_iter().collect())
}

/// Stamps a pack/resolve failure with the dispatching node's id (the
/// `eval.rs` dispatcher rule: impls build errors with `node: 0`).
fn stamped(node: u64, e: EvalError) -> GraphGpuError {
    GraphGpuError::Eval(e.with_node(node))
}

/// Builds (or reuses, per seed) the noise permutation storage buffer:
/// [`umber_graph::nodes::build_perm`]'s bytes widened to `u32`.
fn perm_buffer_for(
    perm_cache: &mut HashMap<i32, wgpu::Buffer>,
    device: &wgpu::Device,
    seed: i32,
) -> wgpu::Buffer {
    if let Some(buf) = perm_cache.get(&seed) {
        return buf.clone();
    }
    let table = umber_graph::nodes::build_perm(seed);
    let words: Vec<u32> = table.iter().map(|b| u32::from(*b)).collect();
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("umber_graph_eval_perm"),
        contents: bytemuck::cast_slice(&words),
        usage: wgpu::BufferUsages::STORAGE,
    });
    perm_cache.insert(seed, buf.clone());
    buf
}

fn pipeline_for<'a>(
    cache: &'a mut PipelineCache<wgpu::ComputePipeline>,
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    entry: &'static str,
) -> &'a wgpu::ComputePipeline {
    // Check-then-insert (not `get_or_insert_with`): the factory borrows
    // `device`/`layout`/`module` while the cache borrows mutably, which a
    // closure cannot express — disjoint field borrows can.
    if !cache.contains(entry) {
        let label = format!("umber_graph_eval_{entry}");
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&label),
            bind_group_layouts: &[Some(bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(&label),
            layout: Some(&pl),
            module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        cache.insert(entry, pipeline);
    }
    cache.get(entry).expect("just inserted")
}

/// Owns the graph-eval compute pipelines and runs dirty subgraphs as
/// per-tile dispatch sequences (see the module docs).
///
/// Created against the app's eframe-owned device/queue (cloned inside —
/// wgpu handles are `Arc`-backed, like [`crate::paint::PaintCompositor`];
/// this crate never constructs its own adapter outside of tests). No
/// adapter-specific feature is required (all stores are `write`-only).
pub struct GraphGpuEvaluator {
    device: wgpu::Device,
    queue: wgpu::Queue,
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipelines: PipelineCache<wgpu::ComputePipeline>,
    fallback_view: wgpu::TextureView,
    #[allow(dead_code)]
    fallback_tex: wgpu::Texture,
    fallback_perm: wgpu::Buffer,
    perm_cache: HashMap<i32, wgpu::Buffer>,
}

impl GraphGpuEvaluator {
    /// Builds the evaluator: compiles the shader module and the shared
    /// bind-group layout, nothing else (pipelines compile lazily on
    /// first dispatch so construction is infallible and cheap).
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("umber_graph_eval_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                crate::graph_shaders::GRAPH_EVAL_SHADER,
            )),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("umber_graph_eval_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<GraphParams>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(4),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<[u32; 2]>() as u64),
                    },
                    count: None,
                },
            ],
        });
        // The 1x1 fallback bound to every unused image slot (the
        // `MESH_SHADER` always-bound convention — only entries that read
        // the slot look at it).
        let fallback_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("umber_graph_eval_fallback"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let fallback_view = fallback_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let fallback_perm = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_graph_eval_fallback_perm"),
            contents: bytemuck::cast_slice(&[0u32; 128]),
            usage: wgpu::BufferUsages::STORAGE,
        });
        Self {
            device,
            queue,
            module,
            layout,
            pipelines: PipelineCache::new(),
            fallback_view,
            fallback_tex,
            fallback_perm,
            perm_cache: HashMap::new(),
        }
    }

    /// Count of compiled pipelines (test observability for the
    /// cache-reuse contract).
    pub fn pipelines_compiled(&self) -> usize {
        self.pipelines.len()
    }

    /// Records one compute pass for `entry` over `dst`'s extent.
    #[allow(clippy::too_many_arguments)]
    fn record_pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        entry: &'static str,
        params: &GraphParams,
        perm: Option<&wgpu::Buffer>,
        src: [&wgpu::TextureView; 3],
        dst: &GpuImage,
    ) {
        let params_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("umber_graph_eval_params"),
                contents: bytemuck::bytes_of(params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let dims_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("umber_graph_eval_dims"),
                contents: bytemuck::cast_slice(&[dst.width, dst.height]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("umber_graph_eval_bind_group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &params_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(src[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(src[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(src[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: perm.unwrap_or(&self.fallback_perm),
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&dst.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &dims_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });
        // Field-disjoint pipeline fetch (see `pipeline_for`): the cache
        // borrows mutably while device/layout/module borrow immutably.
        let pipeline = pipeline_for(
            &mut self.pipelines,
            &self.device,
            &self.layout,
            &self.module,
            entry,
        );
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("umber_graph_eval_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(dst.width.div_ceil(16), dst.height.div_ceil(16), 1);
        }
    }

    /// The single input of a unary node (any wiring name — the
    /// `passthrough`/filter arity rule: exactly one).
    fn single_input(inputs: &[(String, Resolved)], node: u64) -> Result<&Resolved, EvalError> {
        match inputs.len() {
            1 => Ok(&inputs[0].1),
            0 => Err(EvalError::MissingInput {
                node,
                input: "in".into(),
            }),
            n => Err(EvalError::BadParam {
                node,
                param: format!("expected exactly 1 input, got {n}"),
            }),
        }
    }

    /// A required named image input (`mix`'s fg/bg, warp's input, …).
    fn named_input<'a>(
        inputs: &'a [(String, Resolved)],
        node: u64,
        name: &str,
    ) -> Result<&'a Resolved, EvalError> {
        match inputs.iter().find(|(n, _)| n == name) {
            None => Err(EvalError::MissingInput {
                node,
                input: name.into(),
            }),
            Some((_, v)) => Ok(v),
        }
    }

    /// Evaluates one tile of the dirty subgraph: validate → run-set →
    /// topo order → per node: GPU dispatch / CPU registry eval / alias
    /// forward. Returns the FULL output map (fresh + clean-cached),
    /// equal in shape to `eval_graph_cached`; the cache is updated for
    /// every run node (GPU outputs are read back into it, so the next
    /// eval — CPU or GPU — sees identical bytes).
    ///
    /// `tile` supplies the raster resolution for this tile (generator
    /// nodes render at it; the `uniform` `resolution` param may still
    /// override per node, exactly like the CPU engine). Multi-tile
    /// callers loop per tile (see [`plan_dispatches`] for the planned
    /// dispatch set across tiles).
    ///
    /// # Errors
    ///
    /// [`GraphGpuError::Graph`] on topology rejection (before any
    /// dispatch); [`GraphGpuError::Eval`] on param/size/CPU-branch
    /// failures with the dispatching node id stamped (same payloads as
    /// the CPU engine); [`GraphGpuError::NoTiles`] is NOT returned here
    /// (one tile is always given — the empty-list guard lives with the
    /// multi-tile caller); [`GraphGpuError::Readback`] on boundary-copy
    /// failures.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        &mut self,
        graph: &Graph,
        registry: &NodeRegistry,
        inputs: HashMap<String, NodeOutput>,
        ctx: &EvalContext,
        cache: &mut EvalCache,
        dirty: &HashSet<u64>,
        tile: &TileSpec,
    ) -> Result<HashMap<u64, NodeOutput>, GraphGpuError> {
        check_dims(tile.width, tile.height)?;
        graph.validate()?;
        let cached_ids: HashSet<u64> = graph
            .nodes
            .iter()
            .map(|n| n.id)
            .filter(|id| cache.get(*id).is_some())
            .collect();
        let (order, run) = compute_run_set(graph, dirty, &cached_ids)?;

        let mut external = ctx.external.clone();
        external.extend(inputs);
        let eff_ctx = EvalContext {
            resolution: (tile.width, tile.height),
            external,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_graph_eval_encoder"),
            });
        let mut gpu_tex: HashMap<u64, GpuImage> = HashMap::new();
        let mut cpu_out: HashMap<u64, NodeOutput> = HashMap::new();

        for id in &order {
            let node = node_by_id(graph, *id);
            if !run.contains(id) {
                continue;
            }
            let route = GpuRoute::route(&node.node_def, &node.params);
            match route {
                GpuRoute::Alias => {
                    let in_map = resolve_inputs_gpu(
                        graph,
                        node,
                        &gpu_tex,
                        &cpu_out,
                        cache,
                        &eff_ctx.external,
                    )
                    .map_err(|e| GraphGpuError::Eval(e.with_node(node.id)))?;
                    let single =
                        Self::single_input(&in_map, node.id).map_err(GraphGpuError::Eval)?;
                    match single {
                        Resolved::Gpu(t) => {
                            gpu_tex.insert(*id, t.clone());
                        }
                        Resolved::Cpu(v) => {
                            cpu_out.insert(*id, v.clone());
                            cache.insert(*id, v.clone());
                        }
                    }
                }
                GpuRoute::Cpu(_) => {
                    // A CPU node may consume GPU-produced upstreams: submit
                    // the encoder so far, read the textures back, and keep
                    // recording on a fresh encoder (submit-boundary
                    // crossing — correct, not fast; see module docs).
                    let upstream_is_gpu = graph.edges.iter().any(|e| {
                        e.to == *id && gpu_tex.contains_key(&e.from)
                    }) || node.params.iter().any(|(_, pv)| {
                        matches!(pv, ParamValue::NodeRef(up) if gpu_tex.contains_key(up))
                    });
                    if upstream_is_gpu {
                        let submitted = std::mem::replace(
                            &mut encoder,
                            self.device
                                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                    label: Some("umber_graph_eval_encoder"),
                                }),
                        );
                        self.queue.submit(Some(submitted.finish()));
                    }
                    // `resolve_inputs_gpu` already returns inputs sorted by
                    // name (edges overlaying NodeRefs) — flatten to
                    // `NodeOutput`s, reading back GPU handles now that the
                    // encoder is submitted.
                    let resolved = resolve_inputs_gpu(
                        graph,
                        node,
                        &gpu_tex,
                        &cpu_out,
                        cache,
                        &eff_ctx.external,
                    )
                    .map_err(|e| GraphGpuError::Eval(e.with_node(node.id)))?;
                    let mut in_vec: Vec<(String, NodeOutput)> = Vec::with_capacity(resolved.len());
                    for (name, r) in &resolved {
                        let v = match r {
                            Resolved::Cpu(v) => v.clone(),
                            Resolved::Gpu(t) => {
                                NodeOutput::Image(readback_image(&self.device, &self.queue, t)?)
                            }
                        };
                        in_vec.push((name.clone(), v));
                    }
                    let imp = registry.get(&node.node_def).ok_or_else(|| {
                        GraphGpuError::Eval(EvalError::UnknownNode {
                            node_def: node.node_def.clone(),
                        })
                    })?;
                    let out = imp
                        .eval(in_vec, &node.params, &eff_ctx)
                        .map_err(|e| GraphGpuError::Eval(e.with_node(node.id)))?;
                    cpu_out.insert(*id, out.clone());
                    cache.insert(*id, out);
                }
                GpuRoute::Gpu(op) => {
                    let in_map = resolve_inputs_gpu(
                        graph,
                        node,
                        &gpu_tex,
                        &cpu_out,
                        cache,
                        &eff_ctx.external,
                    )
                    .map_err(|e| GraphGpuError::Eval(e.with_node(node.id)))?;
                    self.eval_gpu_node(&mut encoder, node, op, &in_map, tile, &mut gpu_tex)?;
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));

        // Full output map (fresh + clean-cached) + cache write-back for
        // GPU-produced images.
        let mut outputs: HashMap<u64, NodeOutput> = HashMap::with_capacity(graph.nodes.len());
        for id in &order {
            if let Some(t) = gpu_tex.get(id) {
                let buf = readback_image(&self.device, &self.queue, t)?;
                let out = NodeOutput::Image(buf);
                cache.insert(*id, out.clone());
                outputs.insert(*id, out);
            } else if let Some(v) = cpu_out.get(id) {
                outputs.insert(*id, v.clone());
            } else if let Some(v) = cache.get(*id) {
                outputs.insert(*id, v.clone());
            } else {
                return Err(GraphGpuError::Eval(EvalError::MissingInput {
                    node: *id,
                    input: "<cache>".into(),
                }));
            }
        }
        Ok(outputs)
    }

    /// Dispatches one GPU-routed node: input gathering + param packing +
    /// the op's pass sequence, storing the output texture in `gpu_tex`.
    fn eval_gpu_node(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        node: &Node,
        op: GpuOp,
        in_map: &[(String, Resolved)],
        tile: &TileSpec,
        gpu_tex: &mut HashMap<u64, GpuImage>,
    ) -> Result<(), GraphGpuError> {
        let node_id = node.id;
        // First resolved input's extent, for filter nodes (None for
        // generators, which render at the tile extent).
        let first_extent = |r: &Resolved| -> Result<(u32, u32), GraphGpuError> {
            match r {
                Resolved::Gpu(t) => Ok((t.width, t.height)),
                Resolved::Cpu(NodeOutput::Image(buf)) => Ok((buf.width, buf.height)),
                Resolved::Cpu(other) => Err(stamped(
                    node_id,
                    EvalError::TypeMismatch {
                        node: 0,
                        expected: "Image".into(),
                        got: other.kind().into(),
                    },
                )),
            }
        };
        let input_extent = if in_map.is_empty() {
            None
        } else {
            Some(first_extent(&in_map[0].1)?)
        };

        // The warp entry's `use_map` word is set here (the packer leaves
        // slot `a.w` at 0 — only the dispatcher knows the wiring).
        let mut packed =
            pack_node(op, &node.params, input_extent, tile).map_err(|e| stamped(node.id, e))?;
        if op == GpuOp::Warp && in_map.iter().any(|(n, _)| n == "vector") {
            packed.params.a[3] = 1.0;
        }

        let fb = self.fallback_view.clone();
        let src_view_of = |t: &GpuImage| t.view.clone();

        match op {
            // Generators and fills ignore wiring entirely (mirroring the
            // CPU impls, which never inspect `_inputs`).
            GpuOp::FillUniform
            | GpuOp::NoisePerlin
            | GpuOp::NoiseValue
            | GpuOp::NoiseWorley
            | GpuOp::Gradient
            | GpuOp::Checkerboard
            | GpuOp::Dots
            | GpuOp::Brick => {
                let dst = make_texture(&self.device, packed.width, packed.height);
                let perm = packed
                    .perm_seed
                    .map(|seed| perm_buffer_for(&mut self.perm_cache, &self.device, seed));
                let entry = op.entry_points()[0];
                self.record_pass(
                    encoder,
                    entry,
                    &packed.params,
                    perm.as_ref(),
                    [&fb, &fb, &fb],
                    &dst,
                );
                gpu_tex.insert(node.id, dst);
                Ok(())
            }
            // Single-input point/kernel ops.
            GpuOp::Invert
            | GpuOp::Levels
            | GpuOp::Curves
            | GpuOp::ColorCorrect
            | GpuOp::HsvAdjust => {
                let r = Self::single_input(in_map, node.id).map_err(GraphGpuError::Eval)?;
                let tex =
                    mat_tex(&self.device, &self.queue, r, node.id).map_err(GraphGpuError::Eval)?;
                let dst = make_texture(&self.device, packed.width, packed.height);
                let entry = op.entry_points()[0];
                self.record_pass(
                    encoder,
                    entry,
                    &packed.params,
                    None,
                    [&src_view_of(&tex), &fb, &fb],
                    &dst,
                );
                gpu_tex.insert(node.id, dst);
                Ok(())
            }
            GpuOp::EdgeDetect => {
                let r = Self::single_input(in_map, node.id).map_err(GraphGpuError::Eval)?;
                let tex =
                    mat_tex(&self.device, &self.queue, r, node.id).map_err(GraphGpuError::Eval)?;
                let dst = make_texture(&self.device, packed.width, packed.height);
                self.record_pass(
                    encoder,
                    ENTRY_EDGE_DETECT,
                    &packed.params,
                    None,
                    [&src_view_of(&tex), &fb, &fb],
                    &dst,
                );
                gpu_tex.insert(node.id, dst);
                Ok(())
            }
            GpuOp::Mix => {
                let fg = Self::named_input(in_map, node.id, "fg").map_err(GraphGpuError::Eval)?;
                let bg = Self::named_input(in_map, node.id, "bg").map_err(GraphGpuError::Eval)?;
                let fg_tex =
                    mat_tex(&self.device, &self.queue, fg, node.id).map_err(GraphGpuError::Eval)?;
                let bg_tex =
                    mat_tex(&self.device, &self.queue, bg, node.id).map_err(GraphGpuError::Eval)?;
                if fg_tex.width != bg_tex.width || fg_tex.height != bg_tex.height {
                    return Err(stamped(
                        node.id,
                        EvalError::BadParam {
                            node: 0,
                            param: "fg/bg size mismatch".into(),
                        },
                    ));
                }
                let dst = make_texture(&self.device, packed.width, packed.height);
                self.record_pass(
                    encoder,
                    ENTRY_MIX,
                    &packed.params,
                    None,
                    [&src_view_of(&fg_tex), &src_view_of(&bg_tex), &fb],
                    &dst,
                );
                gpu_tex.insert(node.id, dst);
                Ok(())
            }
            GpuOp::Triplanar => {
                let mut ts = Vec::with_capacity(3);
                for name in ["x", "y", "z"] {
                    let r =
                        Self::named_input(in_map, node.id, name).map_err(GraphGpuError::Eval)?;
                    ts.push(
                        mat_tex(&self.device, &self.queue, r, node.id)
                            .map_err(GraphGpuError::Eval)?,
                    );
                }
                if ts[0].width != ts[1].width
                    || ts[0].height != ts[1].height
                    || ts[0].width != ts[2].width
                    || ts[0].height != ts[2].height
                {
                    return Err(stamped(
                        node.id,
                        EvalError::BadParam {
                            node: 0,
                            param: "x/y/z size mismatch".into(),
                        },
                    ));
                }
                let dst = make_texture(&self.device, packed.width, packed.height);
                self.record_pass(
                    encoder,
                    ENTRY_TRIPLANAR,
                    &packed.params,
                    None,
                    [
                        &src_view_of(&ts[0]),
                        &src_view_of(&ts[1]),
                        &src_view_of(&ts[2]),
                    ],
                    &dst,
                );
                gpu_tex.insert(node.id, dst);
                Ok(())
            }
            GpuOp::Warp => {
                let r = Self::named_input(in_map, node.id, "input").map_err(GraphGpuError::Eval)?;
                let tex =
                    mat_tex(&self.device, &self.queue, r, node.id).map_err(GraphGpuError::Eval)?;
                let map_view = match in_map.iter().find(|(n, _)| n == "vector") {
                    None => None,
                    Some((_, mv)) => {
                        let mt = mat_tex(&self.device, &self.queue, mv, node.id)
                            .map_err(GraphGpuError::Eval)?;
                        if mt.width != tex.width || mt.height != tex.height {
                            return Err(stamped(
                                node.id,
                                EvalError::BadParam {
                                    node: 0,
                                    param: "vector/input size mismatch".into(),
                                },
                            ));
                        }
                        Some(mt)
                    }
                };
                let dst = make_texture(&self.device, packed.width, packed.height);
                let map_ref = map_view.as_ref();
                self.record_pass(
                    encoder,
                    ENTRY_WARP,
                    &packed.params,
                    None,
                    [&src_view_of(&tex), map_ref.map_or(&fb, |m| &m.view), &fb],
                    &dst,
                );
                gpu_tex.insert(node.id, dst);
                Ok(())
            }
            GpuOp::Blur => {
                let r = Self::single_input(in_map, node.id).map_err(GraphGpuError::Eval)?;
                let tex =
                    mat_tex(&self.device, &self.queue, r, node.id).map_err(GraphGpuError::Eval)?;
                let blur_params = GraphParams::of([packed.params.a[0], 0.0, 0.0, 0.0], [0.0; 4]);
                let tmp = make_texture(&self.device, packed.width, packed.height);
                let dst = make_texture(&self.device, packed.width, packed.height);
                self.record_pass(
                    encoder,
                    ENTRY_BLUR_H,
                    &blur_params,
                    None,
                    [&src_view_of(&tex), &fb, &fb],
                    &tmp,
                );
                self.record_pass(
                    encoder,
                    ENTRY_BLUR_V,
                    &blur_params,
                    None,
                    [&src_view_of(&tmp), &fb, &fb],
                    &dst,
                );
                gpu_tex.insert(node.id, dst);
                Ok(())
            }
            GpuOp::Sharpen => {
                let r = Self::single_input(in_map, node.id).map_err(GraphGpuError::Eval)?;
                let tex =
                    mat_tex(&self.device, &self.queue, r, node.id).map_err(GraphGpuError::Eval)?;
                // `packed.params.a = [amount, radius, _, _]`; the blur
                // stages take a radius-only uniform.
                let blur_params = GraphParams::of([packed.params.a[1], 0.0, 0.0, 0.0], [0.0; 4]);
                let sharp_params = GraphParams::of([packed.params.a[0], 0.0, 0.0, 0.0], [0.0; 4]);
                let tmp = make_texture(&self.device, packed.width, packed.height);
                let blurred = make_texture(&self.device, packed.width, packed.height);
                let dst = make_texture(&self.device, packed.width, packed.height);
                self.record_pass(
                    encoder,
                    ENTRY_BLUR_H,
                    &blur_params,
                    None,
                    [&src_view_of(&tex), &fb, &fb],
                    &tmp,
                );
                self.record_pass(
                    encoder,
                    ENTRY_BLUR_V,
                    &blur_params,
                    None,
                    [&src_view_of(&tmp), &fb, &fb],
                    &blurred,
                );
                self.record_pass(
                    encoder,
                    ENTRY_SHARPEN,
                    &sharp_params,
                    None,
                    [&src_view_of(&tex), &src_view_of(&blurred), &fb],
                    &dst,
                );
                gpu_tex.insert(node.id, dst);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_shaders::GRAPH_ENTRY_POINTS;
    use umber_graph::nodes::{
        register_filter_color_nodes, register_generator_nodes, register_mask_spatial_nodes,
    };
    use umber_graph::{Edge, Node};

    fn full_registry() -> NodeRegistry {
        let mut registry = NodeRegistry::seeded();
        register_generator_nodes(&mut registry);
        register_filter_color_nodes(&mut registry);
        register_mask_spatial_nodes(&mut registry);
        umber_graph::nodes::register_synth_nodes(&mut registry);
        registry
    }

    fn node(id: u64, def: &str, params: Vec<(String, ParamValue)>) -> Node {
        Node {
            id,
            node_def: def.into(),
            params,
            canvas: None,
        }
    }

    fn edge(from: u64, to: u64, input: &str) -> Edge {
        Edge {
            from,
            to,
            input: input.into(),
        }
    }

    /// (b) The hybrid routing table covers every registered node type: no
    /// built-in def falls through to the plugin catch-all (which would
    /// silently CPU-eval a node the table claims is GPU-routed, or vice
    /// versa — either way the table lies).
    #[test]
    fn routing_covers_every_registered_node_def() {
        let registry = full_registry();
        let defs = registry.node_defs();
        assert_eq!(defs.len(), 25, "unexpected registry size: {defs:?}");
        for def in defs {
            let route = GpuRoute::route(def, &[]);
            assert!(
                !matches!(route, GpuRoute::Cpu(CpuPath::PluginGuest)),
                "{def:?} must have an explicit table row, not the plugin catch-all"
            );
        }
    }

    #[test]
    fn routing_sends_unknown_defs_to_the_plugin_path() {
        // WASM guests and typos evaluate through the registry on the CPU
        // (honest fallback, never a dispatch of a missing entry point).
        assert_eq!(
            GpuRoute::route("blur5", &[]),
            GpuRoute::Cpu(CpuPath::PluginGuest)
        );
        assert_eq!(
            GpuRoute::route("nope_missing", &[]),
            GpuRoute::Cpu(CpuPath::PluginGuest)
        );
    }

    #[test]
    fn routing_splits_the_param_dependent_arms() {
        // `uniform` with a color param fills on the GPU; without one it is
        // a scalar constant (no raster exists to dispatch).
        assert_eq!(
            GpuRoute::route(
                "uniform",
                &[("color".into(), ParamValue::Color([1.0, 0.0, 0.0]))]
            ),
            GpuRoute::Gpu(GpuOp::FillUniform)
        );
        assert_eq!(
            GpuRoute::route("uniform", &[("value".into(), ParamValue::Float(1.0))]),
            GpuRoute::Cpu(CpuPath::ScalarConst)
        );
        // The blur tap budget: small radii stay GPU, hostile ones ride the
        // CPU kernel (routing, not validation — same bytes either way).
        let radius = |r: i32| vec![("radius".into(), ParamValue::Int(r))];
        assert_eq!(
            GpuRoute::route("blur", &radius(1)),
            GpuRoute::Gpu(GpuOp::Blur)
        );
        assert_eq!(
            GpuRoute::route("blur", &radius(MAX_GPU_BLUR_RADIUS)),
            GpuRoute::Gpu(GpuOp::Blur)
        );
        assert_eq!(
            GpuRoute::route("blur", &radius(MAX_GPU_BLUR_RADIUS + 1)),
            GpuRoute::Cpu(CpuPath::LargeKernel)
        );
        assert_eq!(
            GpuRoute::route("sharpen", &radius(10_000)),
            GpuRoute::Cpu(CpuPath::LargeKernel)
        );
        // A mistyped radius routes GPU and fails honestly at pack time
        // (mirroring the CPU node's BadParam — never silent).
        assert_eq!(
            GpuRoute::route("blur", &[("radius".into(), ParamValue::Float(1.0))]),
            GpuRoute::Gpu(GpuOp::Blur)
        );
        // The documented CPU rows.
        assert_eq!(
            GpuRoute::route("image_asset", &[]),
            GpuRoute::Cpu(CpuPath::AssetLoad)
        );
        assert_eq!(
            GpuRoute::route("flood_fill", &[]),
            GpuRoute::Cpu(CpuPath::GlobalPass)
        );
        assert_eq!(
            GpuRoute::route("histogram_match", &[]),
            GpuRoute::Cpu(CpuPath::GlobalPass)
        );
        assert_eq!(
            GpuRoute::route("mesh_map_generator", &[]),
            GpuRoute::Cpu(CpuPath::External)
        );
        assert_eq!(
            GpuRoute::route("anchor", &[]),
            GpuRoute::Cpu(CpuPath::External)
        );
        assert_eq!(GpuRoute::route("passthrough", &[]), GpuRoute::Alias);
    }

    /// (d) The pipeline cache compiles each entry once and reuses it:
    /// the factory call count is the contract (the device-backed half of
    /// this lives in the `gpu` golden below, which asserts the compiled
    /// count across two evaluates).
    #[test]
    fn pipeline_cache_compiles_once_per_entry_and_reuses() {
        use std::cell::Cell;
        let calls = Cell::new(0usize);
        let mut cache: PipelineCache<String> = PipelineCache::new();
        assert!(cache.is_empty());
        let first = cache.get_or_insert_with("op_invert", || {
            calls.set(calls.get() + 1);
            "pipe-a".to_string()
        });
        assert_eq!(first, "pipe-a");
        let second = cache.get_or_insert_with("op_invert", || {
            calls.set(calls.get() + 1);
            "pipe-b".to_string()
        });
        assert_eq!(second, "pipe-a", "second fetch must reuse, not rebuild");
        assert_eq!(calls.get(), 1, "factory runs exactly once per entry");
        cache.get_or_insert_with("fill_uniform", || {
            calls.set(calls.get() + 1);
            "pipe-c".to_string()
        });
        assert_eq!(calls.get(), 2);
        assert_eq!(cache.len(), 2);
        assert!(cache.contains("op_invert"));
        assert!(!cache.contains("op_mix"));
        assert_eq!(cache.get("op_mix"), None);
    }

    /// Every shader entry point is reachable from exactly one op, and
    /// every op maps into the shader table (the cache key contract: a
    /// `pipeline_for` miss on a misspelled entry would compile-fail on
    /// device, so pin both directions statically).
    #[test]
    fn gpu_ops_cover_the_shader_entry_table_both_ways() {
        let ops = [
            GpuOp::FillUniform,
            GpuOp::NoisePerlin,
            GpuOp::NoiseValue,
            GpuOp::NoiseWorley,
            GpuOp::Gradient,
            GpuOp::Checkerboard,
            GpuOp::Dots,
            GpuOp::Brick,
            GpuOp::Invert,
            GpuOp::Levels,
            GpuOp::Curves,
            GpuOp::Mix,
            GpuOp::ColorCorrect,
            GpuOp::HsvAdjust,
            GpuOp::Triplanar,
            GpuOp::EdgeDetect,
            GpuOp::Warp,
            GpuOp::Blur,
            GpuOp::Sharpen,
        ];
        let mut from_ops: Vec<&str> = ops
            .iter()
            .flat_map(|op| op.entry_points())
            .copied()
            .collect();
        from_ops.sort_unstable();
        let mut table: Vec<&str> = GRAPH_ENTRY_POINTS.to_vec();
        table.sort_unstable();
        assert_eq!(from_ops, table, "ops <-> shader entries must agree exactly");
        assert_eq!(GpuOp::Blur.entry_points(), &[ENTRY_BLUR_H, ENTRY_BLUR_V]);
        assert_eq!(
            GpuOp::Sharpen.entry_points(),
            &[ENTRY_BLUR_H, ENTRY_BLUR_V, ENTRY_SHARPEN]
        );
    }

    /// (c) The dirty-set → dispatch-set mapping: dirty seeds, downstream
    /// propagates, the tile list crosses, topo order holds per tile.
    #[test]
    fn plan_maps_dirty_through_downstream_across_tiles() {
        // Chain 1 -> 2 -> 3 (all GPU-routed: uniform fill -> invert x2).
        let red = vec![("color".into(), ParamValue::Color([1.0, 0.0, 0.0]))];
        let mut g = Graph::new();
        g.add_node(node(1, "uniform", red));
        g.add_node(node(2, "invert", vec![]));
        g.add_node(node(3, "invert", vec![]));
        g.add_edge(edge(1, 2, "in"));
        g.add_edge(edge(2, 3, "in"));
        let tiles = [TileSpec::new(1001, 8, 8), TileSpec::new(1002, 8, 8)];

        // Only the tail dirty, everything cached: just the tail per tile.
        let cached: HashSet<u64> = [1, 2, 3].into_iter().collect();
        let plan =
            plan_dispatches(&g, &[3].into_iter().collect(), &cached, &tiles).expect("chain plans");
        assert_eq!(
            plan,
            vec![
                PlannedDispatch {
                    node: 3,
                    tile: 1001,
                    route: GpuRoute::Gpu(GpuOp::Invert),
                },
                PlannedDispatch {
                    node: 3,
                    tile: 1002,
                    route: GpuRoute::Gpu(GpuOp::Invert),
                },
            ]
        );

        // The head dirty: the whole downstream re-runs, topo order per tile.
        let plan =
            plan_dispatches(&g, &[1].into_iter().collect(), &cached, &tiles).expect("chain plans");
        let nodes_1001: Vec<u64> = plan
            .iter()
            .filter(|d| d.tile == 1001)
            .map(|d| d.node)
            .collect();
        let nodes_1002: Vec<u64> = plan
            .iter()
            .filter(|d| d.tile == 1002)
            .map(|d| d.node)
            .collect();
        assert_eq!(nodes_1001, vec![1, 2, 3]);
        assert_eq!(nodes_1002, vec![1, 2, 3]);
        assert_eq!(plan.len(), 6);
        assert_eq!(plan[0].route, GpuRoute::Gpu(GpuOp::FillUniform));
    }

    #[test]
    fn plan_treats_cache_misses_as_dirty_and_skips_clean() {
        // Diamond 1 -> {2, 3} -> 4: dirty {2} with a warm cache runs 2
        // and downstream 4 only (1 and 3 stay cached).
        let mut g = Graph::new();
        g.add_node(node(
            1,
            "uniform",
            vec![("color".into(), ParamValue::Color([0.0, 1.0, 0.0]))],
        ));
        g.add_node(node(2, "invert", vec![]));
        g.add_node(node(3, "invert", vec![]));
        g.add_node(node(4, "invert", vec![]));
        g.add_edge(edge(1, 2, "in"));
        g.add_edge(edge(1, 3, "in"));
        g.add_edge(edge(2, 4, "in"));
        g.add_edge(edge(3, 4, "in"));
        let tiles = [TileSpec::tile_1001(8, 8)];

        let cached: HashSet<u64> = [1, 2, 3, 4].into_iter().collect();
        let plan = plan_dispatches(&g, &[2].into_iter().collect(), &cached, &tiles)
            .expect("diamond plans");
        let nodes: Vec<u64> = plan.iter().map(|d| d.node).collect();
        assert_eq!(nodes, vec![2, 4], "only 2 and downstream 4 re-run");

        // Cold cache with an empty dirty set still runs everything (a
        // miss is dirty) — partial caches can never serve stale data.
        let plan = plan_dispatches(&g, &HashSet::new(), &HashSet::new(), &tiles)
            .expect("cold cache plans");
        let nodes: Vec<u64> = plan.iter().map(|d| d.node).collect();
        assert_eq!(nodes, vec![1, 2, 3, 4]);

        // A clean node with a miss downstream-adjacent: dirty {4} runs
        // only 4 (nothing is downstream of it).
        let plan =
            plan_dispatches(&g, &[4].into_iter().collect(), &cached, &tiles).expect("tail plans");
        let nodes: Vec<u64> = plan.iter().map(|d| d.node).collect();
        assert_eq!(nodes, vec![4]);
    }

    #[test]
    fn pack_rejects_the_same_bad_params_as_the_cpu_nodes() {
        let tile = TileSpec::tile_1001(8, 8);
        let bad = |op: GpuOp, params: &[(String, ParamValue)], input: Option<(u32, u32)>| {
            pack_node(op, params, input, &tile).expect_err("must fail")
        };
        // levels: degenerate range + negative gamma.
        bad(
            GpuOp::Levels,
            &[
                ("in_low".into(), ParamValue::Float(0.5)),
                ("in_high".into(), ParamValue::Float(0.5)),
            ],
            Some((4, 4)),
        );
        bad(
            GpuOp::Levels,
            &[("gamma".into(), ParamValue::Float(-1.0))],
            Some((4, 4)),
        );
        // mix: out-of-range factor.
        bad(
            GpuOp::Mix,
            &[("factor".into(), ParamValue::Float(1.5))],
            Some((4, 4)),
        );
        // curves: non-monotonic x + missing point.
        bad(
            GpuOp::Curves,
            &[
                ("curve_p0".into(), ParamValue::Vec2([0.0, 0.0])),
                ("curve_p1".into(), ParamValue::Vec2([0.5, 0.5])),
                ("curve_p2".into(), ParamValue::Vec2([0.5, 0.8])),
                ("curve_p3".into(), ParamValue::Vec2([1.0, 1.0])),
            ],
            Some((4, 4)),
        );
        bad(GpuOp::Curves, &[], Some((4, 4)));
        // triplanar: zero-sum weights.
        bad(
            GpuOp::Triplanar,
            &[("weights".into(), ParamValue::Vec3([0.0, 0.0, 0.0]))],
            Some((4, 4)),
        );
        // fill: mistyped color + hostile resolution.
        bad(GpuOp::FillUniform, &[], None);
        bad(
            GpuOp::FillUniform,
            &[
                ("color".into(), ParamValue::Color([1.0, 0.0, 0.0])),
                ("resolution".into(), ParamValue::Vec2([0.0, 4.0])),
            ],
            None,
        );
        // filters with no resolved input.
        bad(GpuOp::Invert, &[], None);
        // blur clamps negative radii to a copy (the documented CPU rule),
        // it does not error.
        let packed = pack_node(
            GpuOp::Blur,
            &[("radius".into(), ParamValue::Int(-3))],
            Some((4, 4)),
            &tile,
        )
        .expect("negative radius clamps");
        assert_eq!(packed.params.a[0], 0.0);
        assert_eq!((packed.width, packed.height), (4, 4));
    }

    /// The device-backed golden: uniform-red fill followed by invert must
    /// read back byte-exact (both ops are quantization-clean: 0/1/255
    /// round-trip through normalized f32 exactly), and a second evaluate
    /// must reuse the compiled pipelines, not rebuild them.
    #[cfg(feature = "gpu")]
    #[test]
    fn gpu_fill_then_invert_reads_back_exact_and_reuses_pipelines() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut evaluator = GraphGpuEvaluator::new(device.clone(), queue.clone());
        assert!(evaluator.pipelines_compiled() == 0);

        let mut g = Graph::new();
        g.add_node(node(
            1,
            "uniform",
            vec![("color".into(), ParamValue::Color([1.0, 0.0, 0.0]))],
        ));
        g.add_node(node(2, "invert", vec![]));
        g.add_edge(edge(1, 2, "in"));
        let mut registry = NodeRegistry::seeded();
        register_filter_color_nodes(&mut registry);
        let tile = TileSpec::new(1001, 4, 4);
        let ctx = EvalContext::new(4, 4);
        let mut cache = EvalCache::new();
        let dirty: HashSet<u64> = [1, 2].into_iter().collect();

        let out = evaluator
            .evaluate(
                &g,
                &registry,
                HashMap::new(),
                &ctx,
                &mut cache,
                &dirty,
                &tile,
            )
            .expect("gpu eval works");
        // Red fills, then inverts to cyan.
        match &out[&1] {
            NodeOutput::Image(buf) => {
                assert_eq!((buf.width, buf.height), (4, 4));
                assert!(buf.data.chunks_exact(4).all(|px| px == [255, 0, 0, 255]));
            }
            other => panic!("expected an Image, got {other:?}"),
        }
        match &out[&2] {
            NodeOutput::Image(buf) => {
                assert_eq!((buf.width, buf.height), (4, 4));
                assert!(
                    buf.data.chunks_exact(4).all(|px| px == [0, 255, 255, 255]),
                    "inverted red must be cyan, got {:?}",
                    buf.pixel(0, 0)
                );
            }
            other => panic!("expected an Image, got {other:?}"),
        }
        assert!(
            cache.get(2).is_some(),
            "gpu outputs write back into the shared cache"
        );
        assert_eq!(
            evaluator.pipelines_compiled(),
            2,
            "fill + invert compile once"
        );

        // Second eval, tail-only dirty (the head serves from cache): same
        // bytes, no new pipelines.
        let out2 = evaluator
            .evaluate(
                &g,
                &registry,
                HashMap::new(),
                &ctx,
                &mut cache,
                &[2].into_iter().collect(),
                &tile,
            )
            .expect("second gpu eval works");
        assert_eq!(out2, out, "cached-head re-eval must agree byte-exact");
        assert_eq!(
            evaluator.pipelines_compiled(),
            2,
            "pipelines reuse across evals"
        );
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
