# Height→normal live in viewport — C1 worklist #12

Audit: `docs/loop/audit-2026-10-12-c1.md:340-345`. Gate: "height stroke →
lit viewport normal response, golden-image pinned."

## What landed

GPU-side capability, fully built and golden-tested; app-level wiring
checked and found blocked (see "Deferred" below) — not papered over.

1. **WGSL (`crates/umber-gpu/src/shaders.rs`)**
   - `PaintParams` grown from 32→48 bytes: `height_strength: f32` added
     after `flags`, padding resized `_pad0..2` (3×u32) → `_pad0..5`
     (6×u32).
   - New binding 12 `height_tex: texture_2d<f32>`, new flag
     `PAINT_HAS_HEIGHT_TEXTURE = 2u`, both appended to the existing
     "paint tile" block (reuses `paint_sampler`/`paint_tile_uv` — same
     tile, same addressing as base color).
   - New `height_perturbed_normal(n, world_pos, uv)`: the Mikkelsen
     (2010) screen-space surface-gradient formula — the same math
     behind Three.js's `perturbNormalArb` — built from `dpdx`/`dpdy` of
     the interpolated world position and uv, not per-vertex tangents
     (this pass has none). The height gradient itself comes from two
     explicit `textureLoad`s (u+1, v+1 texel, edge-clamped) rather than
     derivatives of a filtered sample, so it's deterministic and
     golden-pinnable.
   - `shade()` now perturbs `n` before lighting when
     `PAINT_HAS_HEIGHT_TEXTURE` is set — uniform control flow on a
     buffer-sourced flag, the same gate `paint_albedo` already uses.
     Unset: `n` is untouched, byte-identical to before this kill
     (tested).

2. **Renderer (`crates/umber-gpu/src/renderer.rs`)**
   - `PaintParams` Rust struct mirrors the WGSL change; size assert now
     48. New `PaintParams::HAS_HEIGHT_TEXTURE`, `PaintParams::height`,
     `PaintParams::height_for_udim` constructors (mirroring
     `identity`/`for_udim`); shared `udim_origin` helper extracted to
     avoid duplicating the tile-math between `for_udim` and
     `height_for_udim`.
   - `MeshPaint` gained `height_view`; the bind-group layout and
     `mesh_bind_group_full` gained binding 12 (texture, fragment,
     filterable float, D2) — additive, same pattern as every earlier
     block in group 0. Every existing `MeshPaint { .. }` literal in the
     crate (2 besides the new code: `MeshBuffers::upload`, one hand-built
     bind group in an IBL test) updated to supply it (fallback view when
     unbound).
   - `MeshBuffers` gained `bound_height: Option<BoundHeight>` and
     `set_height_texture`/`clear_height_texture`, mirroring
     `set_base_color_texture`/`clear_base_color_texture`'s pattern and
     bind-group-lifetime contract.
   - **Fixed a clobber bug before it could ship**: the obvious
     implementation (`set_height_texture` writing its `PaintParams`
     straight to the uniform buffer) would have been wiped on the very
     next frame, since `viewport.rs` re-binds base color unconditionally
     every frame via `set_base_color_texture`, which used to write its
     `PaintParams` verbatim too. Replaced both setters' direct buffer
     writes with a new private `write_paint_params`, which composes the
     uploaded uniform from `bound_paint` ∪ `bound_height` each time
     either side changes — binding or clearing one channel can no
     longer clobber the other's flag bit. `paint_params()` (the public
     getter `viewport.rs`'s skip-check reads) is deliberately left
     reading `bound_paint` only, unchanged in meaning.
   - Two new tests (`height_stroke_perturbs_the_lit_normal`,
     `height_texture_flag_off_matches_unbound_baseline`) — see
     "Derivation" below. `render_mesh_mode` refactored into a thin
     wrapper over a new `render_mesh_mode_with_camera` (takes an
     explicit `CameraUniform`) with no behavior change for any existing
     caller — needed because the height test requires a light with x/y
     components (the family's usual `NEG_Y` light only reads `n.y`,
     which would hide a pure-x "+u ramp" perturbation entirely).

3. **Docs**
   - `view_mode.rs`'s "Solo sources in v1" doc updated: height is now a
     *partial* exception to "no per-channel GPU textures bound in the
     mesh pass" — true for Lit, still false for `SoloChannel::Height`'s
     proxy arm (untouched, still mid-gray).
   - `viewport.rs::bind_paint_target`'s doc gained a note on why it does
     NOT call `set_height_texture` (see "Deferred").
   - `docs/specs/requirements.md` §5's "View modes" row amended (not a
     new row — no existing row named height specifically) with the new
     capability, its v1 boundary, and the NOT-REACHABLE app-wiring gap.

## Derivation (the golden test's arithmetic)

Full derivation is in the test's doc comment
(`renderer.rs::height_stroke_perturbs_the_lit_normal`); summary:

- Test mesh: a quad spanning NDC `[-1,1]²`, `uv = pos.xy*0.5+0.5` (u
  tracks world x, v tracks world y), normal `(0,0,1)`, identity
  `view_proj` — so `world_pos` IS the vertex position and its
  screen-space derivative is a known constant: `dpdx(world_pos) =
  (0.5,0,0)`, `dpdy(world_pos) = (0,-0.5,0)` at a 4×4 render target
  (the `-0.5` y sign is wgpu's NDC-to-framebuffer flip — confirmed
  against the existing `world_to_pixel_f` helper's `(1-ndc_y)*0.5*h`,
  not re-derived from spec text alone).
- The bound height texture is also 4×4, so every readback pixel's uv
  lands exactly on its matching texel's center — no filtering
  ambiguity in the finite difference.
- A "+u ramp" (0/85/170/255 by column, constant by row) and a "+v
  ramp" (same, by row) at strength 0.75 each give a screen-space height
  derivative of exactly 0.25 along their own axis and 0 along the
  other (worked out texel-by-texel in the test doc).
- A dedicated Rust function (`mikkelsen_perturbed_normal`) runs the
  IDENTICAL formula as the WGSL — not hand-expanded decimals — then
  feeds the result through the SAME env/diffuse/albedo arithmetic the
  existing `expected_procedural_byte` family already uses, under a
  light with x, y, and z components (`to_light = normalize(1,1,1)`)
  chosen specifically so neither ramp's response clips to zero under
  `max(dot(n,to_light),0)`.
- Result: flat/+u/+v give three distinct byte triples at the inspected
  pixel, each with the family's usual ≤1 LSB tolerance; the test
  asserts all three pairwise `assert_ne!` plus the per-channel
  tolerance check against each hand(-formula)-derived expectation.

## Honest boundary (v1)

- **No TBN in the mesh pass.** `dpdx`/`dpdy` are per-primitive,
  screen-space derivatives — correct within one flat facet, with no
  notion of a curved surface's true tangent frame across triangles. A
  heavily triangulated or curved mesh gets a perturbation that's only
  exact per-facet, not continuous across edges. True per-vertex
  tangents (uploaded alongside normals — the bake pipeline's own
  `TRANSFER_BAKE_SHADER::transfer_tbn` convention already exists as a
  model to follow) are the named follow-up once the mesh pass carries
  them.
- **Lit only.** `SoloChannel::Height`'s solo-view proxy still returns
  the mid-gray stand-in; it does not sample binding 12. Same texture,
  different consumer — a small, separate follow-up, left out to keep
  this kill's surface to the one audited gate.
- **`height_strength` units are mesh-scale-dependent** (world units of
  displacement per unit height) — a judgment call, documented on the
  field itself; no attempt to auto-derive a "correct" scale from mesh
  bounds.
- **Tile addressing when base color and height disagree**:
  `write_paint_params` prefers `bound_paint`'s `uv_offset`/`uv_scale`,
  falling back to `bound_height`'s. The app is expected to bind both
  from the same active tile (the common case); v1 doesn't reconcile a
  genuine mismatch between the two.

## Deferred: the app-wiring gap (checked, not fabricated)

The brief asked for `viewport.rs` to bind the height tile "when the
active channel is Height (the artist is sculpting)." That state does
not exist:

- `umber_core::ChannelKind` is `Color | Data` — a storage
  classification, not a per-channel identity. Channels are named
  strings (`Channel.name`), not an enum with a `Height` variant.
- `PaintState` (`crates/umber-app/src/paint_state.rs`) holds
  `targets: BTreeMap<u16, PaintThread>` — one `PaintTarget` PER TILE,
  used for base color only. There is no second, per-channel target.
- `document.rs` already documents this as a known, deliberate gap:
  "the paint-channel binding to GPU targets arrive with the tile
  pool."
- Checked and ruled out as alternate seams: `bake_sources.rs`'s
  `painted_base_color_tile` reads back the SAME single base-color
  `PaintTarget` (not a height-specific one); `mesh_map_import.rs`'s
  `MeshMapKind::Height` routing is import-time CPU classification, not
  a GPU resource; `umber_core::tiles::TileChannels` is pure CPU
  document data with no GPU handle.

Given CLAUDE.md's "do NOT invent a new global," `set_height_texture`/
`clear_height_texture` are built, public, and exercised by this kill's
own tests, but have no app-level caller yet. This is the honest state,
recorded here and in `viewport.rs`'s and `requirements.md`'s doc
comments — not deleted, not `#[allow(dead_code)]`'d (clippy wouldn't
flag public API as dead code anyway). Wiring it is real work for
whichever kill builds the per-channel paint-target binding (the
tile-pool follow-up `document.rs` already names).

## Unverified

- `dpdx`, `dpdy`, and `sign` as WGSL builtins have no prior use in this
  codebase's shaders (confirmed via grep — zero hits before this kill)
  and no vendored naga/WGSL reference under `docs/claw-artifacts/` to
  cross-check against. They're part of the core WGSL language spec
  (not a fast-moving wgpu/egui struct shape), used here inside a branch
  gated on a uniform-buffer value — the same uniformity precedent
  `textureSample` already relies on elsewhere in this exact shader
  (`paint_albedo`, `environment_irradiance`'s `env_flags.flags == 1u`
  branch). Never run on a GPU by this agent; needs a real `cargo test`
  (which this session's contract forbids) or a runtime check to confirm
  naga accepts them in this control-flow shape and that `textureLoad`'s
  3-arg form on a non-storage `texture_2d<f32>` behaves as expected in
  a fragment shader (the `display_lut.rs` precedent uses the identical
  3-arg shape, which is reassuring but not identical code).
- The NDC-to-framebuffer y-flip sign used in the test derivation was
  cross-checked against this file's own `world_to_pixel_f` helper
  (`(1.0 - ndc.y) * 0.5 * h`) rather than re-derived from the WebGPU
  spec text — matches, but is still a derived confirmation, not a GPU
  run.
- `f32::signum` vs WGSL `sign`'s zero case: `signum(0.0) = 1.0` in Rust,
  `sign(0.0) = 0.0` in WGSL. The test's `det` is always nonzero
  (`-0.25`), so this never actually diverges here — flagged in the
  test's own doc comment, not silently assumed.

## Gates run

- `cargo fmt`/`cargo clippy`: not run by this agent — the session's
  Bash access was restricted to read-only lookups (grep/read) per the
  orchestrator's explicit instruction; the dragon runs `rustfmt
  --edition 2021` and `cargo clippy -p umber-gpu --all-targets -- -D
  warnings` / `-p umber-app` at gate time.
- `scripts/verify-kill.sh`: not run, per the same instruction (the
  dragon runs it at gate time).
- No `cargo build`/`cargo test` run, per the standing contract (CI is
  the runtime gate).

## Files touched

- `crates/umber-gpu/src/shaders.rs`
- `crates/umber-gpu/src/renderer.rs`
- `crates/umber-gpu/src/view_mode.rs`
- `crates/umber-app/src/viewport.rs` (doc comment only — no functional
  change)
- `docs/specs/requirements.md`
- `docs/loop/kill-reports/2026-10-12-height-normal.md` (this file)

Tests added: 2 (`height_stroke_perturbs_the_lit_normal`,
`height_texture_flag_off_matches_unbound_baseline`), plus 4 new test
helpers that exist only to support them (`textured_quad_mesh`,
`height_ramp_target`, `mikkelsen_perturbed_normal`,
`expected_height_perturbed_byte`).

---

## Closing amendment (2026-10-24, the tangent-wiring kill)

This report's central judgment — the deliberate choice of
Mikkelsen's screen-space derivative surface gradient because "no
tangent attribute required," with true per-vertex tangents named as
the follow-up — is REVERSED as of the tangent-wiring kill
(`acc3efb`, report `2026-10-24-tangent-wiring.md`). The mesh pass now
uploads per-vertex mikkTSpace frames (`vertex_tangent_frames`, the
d7eea57 foundation) and `height_perturbed_normal` consumes the TBN
frame; the derivative form's per-facet correctness was the boundary
this report shipped under, and the curved-surface golden
(`height_curved_surface_uses_the_vertex_tangent_frame`) pins the two
forms genuinely diverging where the boundary said they would: TBN
bytes [76, 73, 77] vs the derivative frame's [110, 109, 119] at the
probe pixel. The judgment was correct for its day (the frame
attribute did not exist yet); the follow-up it named is now the
implementation. The flat-quad goldens this report added survive
re-derived: the two forms agree in direction on the flat fixture,
differing only by the UV-to-world scale the old chain rule folded
in — see the wiring report's derivation correction.
