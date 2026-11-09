# Kill brief — R-A2 v2c: the decal projector culls back-facing surfaces (row 297)

You are a claw in the Umber workspace. The tree now lives at
**`/mnt/hdd/data/Projects/umber`** (the old path `/home/bessa/Projects/umber`
is a symlink to it — work through either, but the real path is the first).

**Read first:** `docs/loop/briefs/2026-11-09-projection-backface-design.md`
(the design this kill implements). It is binding. Do not re-litigate the
decisions in it; implement them.

## Your surface (touch these files and NOTHING else)

- `crates/umber-mesh/src/uv_gbuffer.rs` — the new mesh seam.
- `crates/umber-mesh/src/lib.rs` — the re-export (the crate's `pub use
  uv_gbuffer::{...}` line).
- `crates/umber-app/src/projection.rs` — the state flag + the cull.
- `crates/umber-app/src/display_panel.rs` — the panel checkbox + the
  `ProjectionRow` borrow field.
- `crates/umber-app/src/main.rs` — wiring the checkbox into the state.
- `assets/i18n/en-US.ftl` and `assets/i18n/pt-BR.ftl` — the two new keys
  (same key names, both catalogs).

**Do NOT touch** `crates/umber-paint/src/paint_state.rs` (the write path
is unchanged), any other crate, `docs/`, `scripts/`, `Cargo.toml`,
`Cargo.lock`, or the `.ftl` files' other keys. If you believe a change
outside this list is required, STOP and say so in your final message
instead of making it.

## The contract

### 1. `crates/umber-mesh/src/uv_gbuffer.rs`

Add ONE public function:

```rust
pub fn world_and_face_normal_at_uv(
    mesh: &MeshData,
    uv: [f32; 2],
) -> Option<([f32; 3], [f32; 3])>
```

- ONE barycentric scan: reuse `uv_to_barycentric`, then read the
  triangle's positions through `mesh.indices` exactly like `world_at_uv`
  does.
- The world point is `lerp3(p0, p1, p2, hit.bary)` — the SAME expression
  and order `world_at_uv` uses today.
- The normal is the triangle's GEOMETRIC normal over the positions IN
  INDEX ORDER: `(p1 - p0).cross(p2 - p0)`, then normalized. Use `f32`
  arithmetic on `[f32; 3]` (this crate has no glam dependency — check
  before assuming; if glam is NOT available, do the cross product by hand
  on `[f32; 3]`).
- A degenerate triangle (cross product length `<= 1e-12` or non-finite)
  → the normal is NOT produced. Choose the return shape that lets the
  caller keep the texel: the cleanest is
  `Option<([f32;3], [f32;3])>` where a degenerate triangle returns
  `None` for the NORMAL but the caller must still be able to write the
  texel. **Recommended shape:** return
  `Option<([f32; 3], Option<[f32; 3]>)>` — the world point plus an
  `Option` normal — so a degenerate normal keeps the texel (the design's
  rule: never silently kill paint on malformed geometry). Pick whichever
  shape you can defend in the doc comment; the REQUIREMENT is that a
  degenerate normal KEEPS the texel and a UV miss writes NOTHING.
- `world_at_uv` is re-implemented as a thin wrapper over the new function
  so there is ONE barycentric scan implementation. Its results must stay
  byte-identical for its existing callers (the existing `world_at_uv`
  tests must pass unchanged).
- Doc comment states: the normal is per-TRIANGLE and matches the GPU's
  `cull_mode: Back` + CCW-front convention (the same expression the
  renderer's tangent-frame builder uses), and that it is NOT the
  per-vertex `MeshData::normals`.

### 2. `crates/umber-app/src/projection.rs`

- `ProjectionState` gains `pub cull_backfaces: bool`, default `true`
  (`Default` impl + the struct's doc comment).
- `ProjectionState::adopt` KEEPS the flag (it is a placement mode, like
  `tiling` and `follow_camera`); `arm` and `disarm` never touch it.
- In the shared per-texel core (`rasterize_with`): after the world point
  is known and BEFORE the texel is written, compute the covering
  triangle's normal and the camera eye, and CULL the texel when
  `dot(n, e - p) < 0`.
  - The eye is `camera.eye()` — the SAME camera the raster projects
    through (the frozen one in the frozen path, the live one in the
    follow path). `OrbitCamera::eye()` already exists in
    `crates/umber-gpu/src/camera.rs`; check its exact name/signature.
  - `dot < 0` → skip the texel (write nothing, not even transparent
    bytes — the buffer is already zero-initialized, so just `continue`).
  - `dot >= 0` → keep. A missing/degenerate normal → keep.
  - Non-finite inputs → keep (never silently kill paint).
- The module doc's "v1 cut line" section MUST be updated: the "No depth
  or backface culling" bullet becomes the v2c reality (backface culling
  landed; depth culling deferred), and the new flag is documented next to
  the tiling/follow bullets in the same style. Name the depth-culling
  deferral explicitly.

### 3. `crates/umber-app/src/display_panel.rs` + `main.rs` + the catalogs

- `ProjectionRow` gains a `cull_backfaces: &mut bool` borrow field (the
  same borrow pattern `tiling` and `follow_camera` use).
- The panel draws a checkbox labelled with the new i18n key, next to the
  Tile / Follow camera checkboxes, with a hover hint key (both catalogs
  get BOTH keys, same key names).
- `main.rs`'s `projection_section` passes `&mut state.cull_backfaces` into
  the row.
- New keys (choose the final names; keep them under the existing
  `overlays.projection-*` namespace and mirror the existing Tile/Follow
  key style):
  - `overlays.projection-cull-backfaces` — the checkbox label.
  - `overlays.projection-cull-backfaces-hint` — the hover hint, stating
    that back-facing surfaces do not take the decal and that unchecking
    restores painting both sides.
- The catalogs are `.ftl` (Fluent). Match the existing `projection-tile`
  / `projection-follow` entries' shape and placement. BOTH catalogs get
  the same keys — the i18n completeness gate checks this.

## Tests you must write (the pins the design names)

Mesh crate (`crates/umber-mesh/src/uv_gbuffer.rs` tests):
- The world point from the new function equals `world_at_uv`'s, byte for
  byte.
- The normal is unit length and points along the expected axis for a
  known triangle.
- Reversing the triangle's winding flips the normal.
- A degenerate (collinear) triangle yields no usable normal (and the
  raster keeps the texel — see the app pin).
- A UV miss yields nothing.

App crate (`crates/umber-app/src/projection.rs` tests):
- A front-facing quad takes the decal — byte-identical to the existing v1
  pin, WITH culling ON (this is the regression guard).
- The same quad with REVERSED winding takes NOTHING with culling ON.
- The same reversed quad takes the decal again with culling OFF (the
  escape hatch restores v1 exactly).
- Tiling ON + culling ON compose (the tiled decal still skips back
  faces).
- Follow-camera ON + culling ON compose.
- `adopt` keeps the flag; `arm`/`disarm` do not change it (the existing
  `adopt_keeps_*` pins are the model — add the new flag to that
  discipline).

Look at the existing tests in both files and FOLLOW their shape and
fixture style (the `quad()` / `head_on()` / `armed()` helpers are already
there).

## Hard rules

- **NO cargo.** No `cargo build`, `cargo test`, `cargo clippy`,
  `cargo fmt`, no `rustfmt` on the whole tree. The build host is shared
  and slow. The dragon runs the battery. Do not try to "verify" by
  building.
- **NEVER commit, push, stash, checkout, reset or clean.** The dragon
  commits. Leave the changes in the working tree.
- **Do not reformat unrelated code.** The tree is `rustfmt`-clean; a
  `cargo fmt` run would touch files outside your surface. Match the
  existing formatting by hand (the file's style is rustfmt's default).
- **No new dependencies.** `Cargo.toml` / `Cargo.lock` are out of bounds.
  This crate uses `f32` arrays, not glam, in `uv_gbuffer.rs` — verify.
- **Do not edit the queue, the ROADMAP, `STATE.md`, `.dragon/`, or the
  kill reports.** The dragon writes those.
- If the design brief conflicts with what the code actually says, the
  CODE wins for the mechanics and you report the deviation in your final
  message (do not silently redefine the contract).

## Final message

Report: the files you changed, the exact public names you added (the
mesh function's signature and the state's field), the i18n key names you
added to BOTH catalogs, the pins you wrote, and any deviation from the
brief with the reason. Be precise — the dragon verifies from the
artifacts, not from your report.
