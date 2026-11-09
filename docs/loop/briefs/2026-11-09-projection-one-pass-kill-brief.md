# Kill brief — R-A2 v2d: the decal raster becomes ONE PASS (row 298)

You are a claw in the Umber workspace. The tree lives at
**`/mnt/hdd/data/Projects/umber`** (the old path `/home/bessa/Projects/umber`
is a symlink to it).

**Read first:** `docs/loop/briefs/2026-11-09-projection-one-pass-design.md`
(binding). It states the measured defect, the fix, and — most important —
the rule that makes the output bytes identical. Do not re-litigate it.

**THIS IS A PERFORMANCE KILL. The output bytes MUST NOT CHANGE.** The
design brief's §3 is the rule you must not get wrong: a triangle that
produces no texel value must WRITE transparent `[0,0,0,0]`, not skip the
texel — otherwise an earlier triangle's colour survives where today's
winner decides.

## Your surface (these files and NOTHING else)

- `crates/umber-mesh/src/uv_gbuffer.rs` — the new public streaming walk.
- `crates/umber-mesh/src/fill.rs` — `rasterize_uv_geometry` delegates to it.
- `crates/umber-mesh/src/lib.rs` — the re-export (if you add a public name).
- `crates/umber-app/src/projection.rs` — the triangle-driven raster.

**Do NOT touch** `crates/umber-paint/**` (the write path is unchanged),
any other crate, `docs/`, `scripts/`, `Cargo.toml`, `Cargo.lock`, or the
`.ftl` catalogs. If you believe a change outside this list is required,
STOP and say so in your final message instead of making it.

## The contract

### 1. `crates/umber-mesh/src/uv_gbuffer.rs` — the new primitive

```rust
pub fn for_each_uv_geometry(
    mesh: &MeshData,
    tile: u16,
    width: u32,
    height: u32,
    filter: impl FnMut(usize) -> bool,
    f: impl FnMut(u32, u32, [f32; 3], [f32; 3]),
)
```

- It IS the existing `for_each_covered_texel_filtered` walk (the crate's
  single containment primitive) plus the per-triangle geometry — do NOT
  write a second containment walk.
- `filter` receives the TRIANGLE index; `false` skips that triangle
  entirely (the shape `for_each_covered_texel_filtered` already has).
- `f` receives `(x, y, world, raw_normal)`:
  - `world` = `lerp3(p0, p1, p2, bary)` — the same expression and order
    `world_at_uv` uses.
  - `raw_normal` = the UNNORMALIZED cross product
    `(p1 - p0) x (p2 - p0)` over the positions IN INDEX ORDER. Yield it
    RAW: the projector and the fill apply DIFFERENT normalization rules
    (the projector: usable only if `len > 1e-12` and finite, else "no
    facing"; the fill: `len > 0.0` → normalized, else `[0,0,0]`), and
    each must keep its own.
- A triangle with non-finite positions is still VISITED (the projector
  needs today's winner semantics — see the design §3). Only `filter` may
  skip it.
- The walk visits triangles in INDEX ORDER (the winner rule depends on
  it).
- Zero-sized targets (`width == 0` or `height == 0`) visit nothing.
- Doc comment states: the walk is the crate's single containment
  primitive, the winner rule (last in index order), that the normal is
  raw and why, and that a filtered-out triangle is not visited.

### 2. `crates/umber-mesh/src/fill.rs` — delegate, do not duplicate

`rasterize_uv_geometry` must be REIMPLEMENTED on top of the new
primitive (its current per-triangle geometry math moves into the
primitive; the function keeps its public signature and its output
byte-identical). Its current filter (skip triangles whose positions are
not all finite) becomes the `filter` argument.

Check `rasterize_uv_geometry`'s existing pins in that file — they must
still pass UNCHANGED. If a pin's expectation depends on the old
implementation's internals rather than its output, adapt the pin and say
so in your report.

### 3. `crates/umber-app/src/projection.rs` — the triangle-driven raster

`rasterize_with` becomes triangle-driven:

```rust
umber_mesh::for_each_uv_geometry(mesh, tile, w, h, |_| true, |x, y, world, raw| {
    // 1. the normal: usable iff len > 1e-12 and finite, else NO facing
    // 2. the backface test (the existing `faces_away`): a culled texel
    //    writes TRANSPARENT (it does NOT skip)
    // 3. project through the camera; behind-the-eye / off-NDC /
    //    NaN -> write TRANSPARENT (it does NOT skip)
    // 4. sample the image (the existing tiling / non-tiling branches)
    //    and write the RGBA
});
```

- The per-texel WORK is unchanged: the same `view_proj`, the same
  `inverse_map`, the same tiling wrap, the same samplers, the same
  `faces_away` test.
- **The write rule (design §3): every path that produces no value writes
  `[0,0,0,0]` to that texel.** Today those paths left the texel at zero
  because nothing else had written it; in the triangle-driven version an
  earlier triangle may have. Writing transparent preserves today's bytes.
- The backface test's semantics are unchanged: `dot(n, eye - p) < 0` culls;
  a MISSING/DEGENERATE normal keeps the texel (malformed geometry never
  silently kills paint); `dot == 0` keeps it.
- The output buffer's shape and size are unchanged (one RGBA8 texel per
  target texel).
- The module docs' "per-texel core" description must be updated to
  describe the triangle-driven pass and to state the measured cost
  (the design brief's §1 numbers) — the current text says the raster
  does "one O(1) lookup chain per texel", which is FALSE and is the
  defect's origin. Correct it in the same kill.

## The pins you must write

### Mesh crate

- The new walk yields the same TEXEL SET and the same WINNER per texel
  as `UvGBuffer::build` / `uv_to_barycentric` on a mesh with OVERLAPPING
  UV islands (last-in-index-order wins).
- `filter` is honoured: a filtered-out triangle contributes no texels
  (pin with the fill's `pos_finite` rule on a mesh with a NaN-position
  triangle).
- WITHOUT a filter, a NaN-position triangle IS visited (its texels are
  yielded) — the projector depends on this.
- The yielded `world` equals `world_at_uv`'s byte for byte, and the
  `raw_normal` equals the manual cross product for a known triangle.
- `rasterize_uv_geometry`'s output is unchanged (its existing pins).

### App crate — the byte-identity contract

Keep the CURRENT per-texel raster in the test module as the reference
implementation (the file already has this shape: `v1_reference` /
`reference_through` are the hand-built references — extend that
pattern). Then pin:

- A MULTI-TRIANGLE mesh: the new raster equals the reference, byte for
  byte, with culling OFF and with culling ON.
- A mesh with OVERLAPPING UV islands: equal bytes.
- A mesh with a NaN-position triangle: equal bytes (today the NaN winner
  blanks the texel; the new one must too).
- A BACK-FACING winner with culling ON: that texel is TRANSPARENT — not
  an earlier triangle's colour. This is the design §3 rule; pin it
  directly (a mesh where a front-facing triangle and a later back-facing
  triangle cover the same texel).
- Tiling ON composes (the v2a pin keeps its meaning), follow-camera ON
  composes (the v2b pin), culling ON composes (the v2c pin).

Look at the existing tests in both files and FOLLOW their shape and
fixture style (the `quad()` / `head_on()` / `armed()` helpers are already
there).

## Hard rules

- **NO cargo.** No `cargo build`, `cargo test`, `cargo clippy`,
  `cargo fmt`. The build host is shared and slow; the dragon runs the
  battery. Do not "verify" by building.
- **NEVER commit, push, stash, checkout, reset or clean.** The dragon
  commits. Leave the changes in the working tree.
- **Do not reformat unrelated code.** The tree is `rustfmt`-clean; a
  `cargo fmt` run would touch files outside your surface. Match the
  existing formatting by hand.
- **No new dependencies.** `Cargo.toml` / `Cargo.lock` are out of bounds.
- **Do not edit** the queue, the ROADMAP, `STATE.md`, `.dragon/`, the
  kill reports, or the i18n catalogs.
- If the design brief conflicts with what the code actually says, the
  CODE wins for the mechanics and you report the deviation in your final
  message (do not silently redefine the contract).

## Final message

Report: the files you changed, the exact public names you added (the
primitive's signature and the re-export), how `rasterize_uv_geometry`
was delegated, the pins you wrote, and any deviation from the brief with
the reason. Be precise — the dragon verifies from the artifacts, not
from your report.
