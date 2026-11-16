# Kill brief: D4 ID sources (multi-source ID bake, baker power row, SPP census D4)

- Status: DESIGN ONLY. Written 2026-11-16 by a Razul-dispatched subagent under kill-lock `razul-d4-id-sources` (docs only, no code touched).
- Census source: `docs/loop/audit-2026-10-24-lens-spp.md:37` — "D1 baker sync / D2 auto-cage / D3 opacity baker / D4 ID sources / D5 ground plane | per census | unchanged".
- ROADMAP scoreboard: `docs/ROADMAP.md:339` — "Baking | PARITY (per-set masked bakes closed; D1 baker sync LANDED 2026-11-11 row 377; D2-D5 power rows open — next-steps list item B)".
- Next-steps list: `docs/loop/next-steps-2026-11-16.md:28` — item B.6 "D4 ID sources (multi-source ID bake). Same loop.".
- Sibling brief structure to mirror: `docs/loop/briefs/2026-11-16-d2-auto-cage.md` (D2 auto-cage, landed brief format) and `docs/loop/briefs/2026-11-16-d3-opacity-baker.md` (D3 opacity baker, landed brief format).
- D1 persistence pattern to mirror: `docs/loop/briefs/2026-11-11-rd1-baker-sync.md` (BakeSync additive fields, `..Default::default()` on load, no project version bump).

## What exists (measured on live tree)

ID baker module (`crates/umber-bake/src/id.rs`):
- `IdFlavor` enum at `:59-70`: `Triangle` (shipped, FNV-1a hash of winning triangle index), `Material` (gated, rejected at runtime — `MeshData` carries only `material_names` palette, no per-triangle assignment).
- `IdBakeParams` at `:74-81`: `flavor: IdFlavor`, `width: u32`, `height: u32`.
- `bake_id_mesh` at `:174-283`: two-pass (UV G-buffer → hash in `ID_BAKE_SHADER`). Returns RGBA8 (`rgb = fnv1a_32(tri)`, `a = 255` covered / `0` uncovered).
- GPU shader: `ID_BAKE_SHADER` at `bake_shaders.rs:1161-1238` — `cs_main` reads `prim_tex` (u32 triangle index), hashes via `hash_triangle(tri)` (FNV-1a in WGSL), writes RGBA8.
- CPU mirror test: `fnv1a_triangle` at `id.rs:294-305` — Rust-side FNV-1a used by kittests to assert GPU output byte-equals CPU mirror (`split_halves_regions_hash_to_their_triangle_index`, `gpu_id_bake_matches_cpu_uv_gbuffer_texel_for_texel`).
- Kittests at `:396-658` (under `#[cfg(feature = "gpu")]`): region hash correctness, uncovered texels = zero, double-bake byte-identical, CPU UV G-buffer parity (texel-for-texel).

Bake kind registry (`umber-bake` lib.rs + `bakes_panel.rs`):
- `BakeMap` enum at `crates/umber-bake/src/lib.rs:100-114` lists 11 maps (`AmbientOcclusion` through `Skew`); **NO multi-source ID variant** (only `Id` exists).
- `BakeSelection` enum at `crates/umber-app/src/bakes_panel.rs:218-246` lists 9 selectable maps (`Ao` through `BentNormal` + `Opacity`); **NO multi-source ID variant**.
- `BakeSettingId` at `:441-456` enumerates settings rows the log jumps to; multi-source ID would need rows (e.g. `IdSources`, `IdSourceWeights`, `IdSourceMasks`).
- `settings_snapshot()` at `:1358-1393` builds the per-baker param map for `BakeSync` — ID source params must be additive here.
- `apply_settings()` at `:1399-1432` loads params back; ID source params must be read with defaults.
- `plan_bake()` at `:348-388` fans out `BakeJob`s; a new `BakeSelection::IdMulti` (or similar) must be handled (single-tile only in v1, like curvature/position/opacity).

Per-set masked bakes context (`per-set-masked-bakes.md` §3):
- Bakes take `(raster mesh, occluder mesh)`; today same argument for self-bakes. Multi-source ID is a self-bake (raster = set submesh, occluder = whole mesh by default, like ID).

Multi-source ID semantics (SPP parity):
- Substance Painter's ID mask baker can combine multiple ID sources (Material ID, Object ID, Vertex ID, custom maps) into one composite ID map with per-source weights and masking. In Umber: per-texel composite ID derived from multiple *sources* (Triangle hash, Material hash when per-triangle material assignment lands, existing baked ID maps sampled as textures, manual uniform colors), blended by per-source weights and masks, output as RGBA8 (RGB = composite hash color, A = coverage). Matches SPP's "ID mask from multiple sources" workflow.

## Dispatch lock list

Lock name: **`razul-d4-id-sources`**

Locked files (exact surface at dispatch):
- `crates/umber-bake/src/id.rs` (extend with multi-source params, CPU reference, new entry point)
- `crates/umber-bake/src/lib.rs` (export new types; add `IdMulti` to `BakeMap`)
- `crates/umber-gpu/src/bake_shaders.rs` (new `ID_MULTI_BAKE_SHADER` constant: plane + mesh-fed entry points)
- `crates/umber-app/src/bakes_panel.rs` (`BakeSelection::IdMulti` variant + `ALL`/`DEFAULT_ON`, `BakeSettingId::IdSources`/`IdSourceWeights`/`IdSourceMasks`, panel UI rows, `settings_snapshot`/`apply_settings`, `plan_bake` handling)
- `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl` (new keys: `bakes.map-id-multi`, `bakes.id-sources`, `bakes.id-source-triangle`, `bakes.id-source-material`, `bakes.id-source-id-map`, `bakes.id-source-manual`, `bakes.id-source-weight`, `bakes.id-source-mask`, `bakes.id-blend-mode`)
- `docs/loop/queue.md` + this brief's kill report

## Build (dispatch-time lock: `razul-d4-id-sources`)

### 1. Extended ID baker module (`id.rs`)

**New types:**
- `IdSource` enum (which source contributes to the composite):
  - `Triangle` — FNV-1a hash of winning triangle index (current ID baker).
  - `Material` — FNV-1a hash of per-triangle material index. **Gated at runtime** with `IdBakeError::MaterialAssignmentUnsupported` (same gate as `IdFlavor::Material`) until per-triangle material assignment lands (wave-4 item 4).
  - `IdMap` — sample an existing baked ID map (texture binding 6, same pattern as opacity's `IdMap` source). Coverage = `id_tex.a > 0.5`, color = `id_tex.rgb`.
  - `Manual` — uniform color (`[r, g, b]` in 0..255).
- `IdSourceParams { source: IdSource, weight: f32, mask: Option<TextureView>, color: [u8; 3] }` — per-source config. `weight` ∈ [0, 1] (normalized blend weight). `mask` = optional coverage mask texture (sampled at texel, `mask.a > 0.5` gates contribution). `color` = manual color for `Manual` source.
- `IdMultiParams { sources: Vec<IdSourceParams>, blend_mode: IdBlendMode, width: u32, height: u32 }` — full multi-source config.
- `IdBlendMode` enum: `Additive` (weighted sum of RGB, clamped to 255), `Max` (per-channel max across sources), `Replace` (first source with coverage wins — painter's "layer stack" order). Default `Additive`.

**CPU reference implementation:**
- `fn hash_source_cpu(mesh: &MeshData, params: &IdMultiParams, tri: u32, bary: (f32, f32), id_map_tex: Option<&[u8]>, id_map_dims: (u32, u32)) -> [u8; 4]` — computes composite color for one texel given winning triangle + barycentrics. Mirrors shader logic exactly (same FNV-1a, same blend math, same IEEE operation order).
- `fn validate_multi(mesh: &MeshData, params: &IdMultiParams) -> Result<(), IdBakeError>` — extends `validate()`: checks at least one source, weights sum > 0, `Material` source rejected if no per-triangle material assignment, `IdMap` texture dimensions match target, manual colors in range.

**New GPU entry point:**
- `pub fn bake_id_multi_mesh(device, queue, mesh, params) -> Result<Vec<u8>, IdBakeError>` — two-pass like `bake_id_mesh`:
  1. UV G-buffer (`bake_uv_gbuffer`) for winning triangle + barycentrics (same as ID baker).
  2. `ID_MULTI_BAKE_SHADER` dispatch: reads `prim_tex` (triangle index), `bary_tex` (barycentrics from position pass — or recomputed in shader from position derivatives), optional `id_map_tex` (binding 6), uniform `IdMultiUniform` (packed source configs). Writes RGBA8 composite.

**Unit tests (CPU-only, no GPU feature):**
- `id_multi_additive_blend_matches_cpu`: two sources (Triangle + Manual), known weights → CPU reference output matches hand-computed blend.
- `id_multi_max_blend_picks_brightest`: `IdBlendMode::Max` with Triangle (hash=dark) + Manual (bright) → output = bright.
- `id_multi_replace_first_wins`: `IdBlendMode::Replace` with Triangle (covered) + Manual → output = Triangle hash (first source wins on coverage).
- `id_multi_id_map_source_samples_coverage`: `IdSource::IdMap` with synthetic ID map texture → covered texels sample ID map RGB, uncovered = 0.
- `id_multi_material_source_rejected_without_assignment`: `IdSource::Material` in params → `validate_multi` returns `MaterialAssignmentUnsupported` without touching GPU.
- `id_multi_weight_zero_excludes_source`: source with `weight = 0.0` contributes nothing (bit-identical to source omitted).
- `id_multi_mask_gates_contribution`: source with mask texture → only texels where mask.a > 0.5 get contribution.

### 2. GPU shader (`ID_MULTI_BAKE_SHADER` in `bake_shaders.rs`)

**Uniform struct (WGSL-aligned, 16-byte aligned):**
```wgsl
struct IdSourceUniform {
  source: u32,      // IdSource enum discriminant
  weight: f32,
  has_mask: u32,    // 0/1
  color: vec4<u32>, // manual color (r,g,b,0) for Manual, else 0
  _pad: u32,        // alignment
}
struct IdMultiUniform {
  source_count: u32,
  blend_mode: u32,  // IdBlendMode discriminant
  width: u32,
  height: u32,
  sources: array<IdSourceUniform, 8>, // max 8 sources (fixed for bind group layout)
}
```
(64 bytes for sources + 16 header = 80 bytes, padded to 16-byte boundary = 80 bytes — acceptable for uniform buffer.)

**Plane entry `cs_main`:**
- Identical dispatch to `ID_BAKE_SHADER` (`dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1)`, `@workgroup_size(8, 8)`).
- Reads `prim_tex` (binding 0, `texture_2d<u32>`) for triangle index.
- For each active source in `sources`:
  - `Triangle`: `hash_triangle(tri)` (reuse existing FNV-1a function).
  - `Material`: **gated** — if enabled, reads `triangle_materials` buffer (binding 7, `buffer<u32>`) at `tri`; if buffer not bound (no material assignment), contributes 0. Same runtime gate as CPU.
  - `IdMap`: samples `id_map_tex` (binding 6, `texture_2d<f32>`) at texel coords; coverage = `sample.a > 0.5`, color = `sample.rgb * 255`.
  - `Manual`: uses `color` from uniform.
  - Applies `weight`, optional `mask` texture (binding 8, `texture_2d<f32>` — shared mask binding, indexed by source).
- Blends per `blend_mode`: `Additive` = `sum(color * weight)` clamped; `Max` = `max(color * weight)` per channel; `Replace` = first source with coverage (source order = array order).
- Output: `vec4(composite_rgb / 255.0, coverage_f32)` → `textureStore` to `Rgba8Unorm` storage texture.

**Mesh-fed entry `cs_main_from_position`:**
- Same as plane entry but reads `position_tex`/`normal_tex` (bindings 2/3) to reconstruct world position/normal if needed (not needed for ID but kept for parity with AO/thickness/opacity pattern).
- `bary_tex` not currently baked — for v1 the plane entry is sufficient (ID baker today uses plane dispatch). Mesh-fed entry is a stub that delegates to plane logic (position pass not used for ID sources).

### 3. Panel wiring (`bakes_panel.rs`)

- Add `IdMulti` to `BakeSelection` (after `Opacity`, before `Height` — low-mesh self-bake). Update `ALL` array (10 entries) and `DEFAULT_ON` (keep 5 — IdMulti is opt-in).
- Add `IdSources`, `IdSourceWeights`, `IdSourceMasks` to `BakeSettingId` (for log jump-to-setting — all three point to the ID multi settings group).
- Settings group (enabled when `IdMulti` checked in Maps group):
  - Source list editor: add/remove/reorder sources (max 8). Each source row: combo `IdSource` (Triangle / Material / ID map / Manual), slider `Weight` (0..1, step 0.01), optional mask texture picker (file dialog → loads as `TextureView`), color picker for `Manual` (RGB).
  - Blend mode combo: `Additive` / `Max` / `Replace`.
- `settings_snapshot()`: for `BakeSelection::IdMulti` emit serialized `IdMultiParams` into the baker param map (using `ParamValue::List` for sources, `ParamValue::Int` for blend mode). Each source serializes as a map with keys `source`, `weight`, `mask_path` (optional string), `color` (array of 3 ints).
- `apply_settings()`: read back with defaults (single `Triangle` source, weight 1.0, `Additive` blend).
- `plan_bake()`: `IdMulti` uses `BakeSource::WholeMesh` for tile 1001 only (like curvature/position/opacity); skipped for other tiles.

### 4. Persistence (R-D1 additive pattern)

- No new `BakeSyncCarry` fields — IdMulti params live inside the per-baker `BTreeMap` that `settings_snapshot`/`apply_settings` already serialize. The `ParamValue` enum (`Int`/`Float`/`Bool`/`String`/`List`/`Map`) covers all needed types.
- Old projects load with zero IdMulti settings (map unchecked, params absent) — `apply_settings` keeps current slider values for missing keys. Re-save writes no IdMulti block when unchecked.

## Kittest-first definition of done

- `id_multi_additive_blend_matches_cpu`: two sources (Triangle + Manual color=[255,0,0]), weights 0.5/0.5, `Additive` → GPU output RGB == CPU reference (FNV-1a(tri) * 0.5 + [127,0,0] clamped), byte-for-byte.
- `id_multi_max_blend_picks_brightest`: Triangle (hash=dark) + Manual (bright), `Max` → output = bright source per channel.
- `id_multi_replace_first_wins`: Triangle covered + Manual covered, `Replace` → output = Triangle hash exactly (first source wins).
- `id_multi_id_map_source_masks_coverage`: bake ID map first (Triangle), then IdMulti with `IdSource::IdMap` weight=1.0 → covered texels read ID map RGB, uncovered read 0; `invert` not applicable (coverage from ID map alpha).
- `id_multi_material_source_rejected_on_gpu_path`: `IdSource::Material` in params + no material assignment buffer bound → GPU path returns `MaterialAssignmentUnsupported` (same error as CPU `validate_multi`).
- `id_multi_weight_zero_is_byte_identical_to_omitted`: source with weight=0.0 → output bit-identical to config without that source.
- `id_multi_mask_gates_contribution`: source with mask texture covering left half → left half shows source color, right half shows other sources only.
- `id_multi_off_is_byte_identical_to_today`: `BakeSelection::IdMulti` unchecked + all other maps checked → full bake plan (jobs, outputs, log) byte-for-byte identical to tree without the IdMulti module (no new jobs, no new log entries, no new files). Proves zero regression when the new baker is not selected.
- `id_multi_double_bake_is_byte_identical`: same mesh + same IdMulti params baked twice → outputs byte-identical (no RNG anywhere in the path).
- `id_multi_cpu_gpu_parity_texel_for_texel`: same mesh baked via CPU reference (`hash_source_cpu` over all texels) and GPU `bake_id_multi_mesh` → every texel byte-identical (extends `gpu_id_bake_matches_cpu_uv_gbuffer_texel_for_texel` to multi-source).

## Rules

- Battery before commit (all under `flock /tmp/umber-cargo.lock` + `UMBER_ASSETS_DIR=<repo root>`):
  - `rustfmt --edition 2021 --check` on touched `.rs`
  - `python3 scripts/check-layers.py`
  - `python3 scripts/scorecard.py --check`
  - `cargo clippy -p umber-bake -p umber-gpu -p umber-app --all-targets -- -D warnings`
  - `cargo test -p umber-bake -p umber-gpu -p umber-app` (0 failed)
- Both `.ftl` catalogs if keys added (ftl completeness is a battery gate).
- No weakened tests; new behavior gets new tests (kittest-first above).
- Edit only the locked surface at dispatch.
- Commit via `.dragon/dcommit.py --kill-lock razul-d4-id-sources --lock <paths> -- -m "..."`; push via `scripts/safe-push.py` in background.

## Rejected alternatives

| Alternative | Reason |
|-------------|--------|
| Separate bake kinds per source + composite pass | Would need N bake kinds + a composite dispatch; one shader with source array avoids N texture allocations, N dilation passes, N export slots. The blend modes are ~20 lines of WGSL. |
| Material ID as a separate bake kind | Material ID is gated on per-triangle assignment (same as current `IdFlavor::Material`). Keeping it as a source in IdMulti means the gate is one place, not two. |
| ID map sampling in a post-process | Would need the ID map baked first, then a second dispatch to sample it. The shader can sample `id_map_tex` inline with zero extra dispatch (same as opacity's `IdMap` source). |
| Dynamic source count (unbounded array) | WGSL uniform buffers need fixed size. Max 8 sources covers every realistic workflow (SPP exposes 4: Material/Object/Vertex/Custom). Fixed array = simpler bind group, no dynamic indexing. |
| Per-tile IdMulti bakes (like AO) | IdMulti is a coverage-composite mask — no hemisphere sampling that benefits from tile windows. Single-tile raster (like curvature/position/opacity) is correct for v1; UDIM support is D5+ work. |
| Vertex ID as a source | Vertex ID needs per-vertex indices interpolated across the triangle (barycentric lerp of vertex index — not a standard attribute). Triangle index is the authoritative region ID; vertex ID adds no masking value over triangle ID. |

## Dragon verification

- Cross-review diff vs this brief, full battery on the landed commit, `ls-remote origin` hash check.
- Kill report at `docs/loop/kill-reports/2026-11-16-d4-id-sources.md` with measured numbers: IdMulti bake time (ms) on 512² mesh vs single-source ID baseline, byte-identity proof for `IdMulti` unchecked, CPU-GPU parity texel-for-texel on 3 source configurations.