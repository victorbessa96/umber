# Kill report — per-layer pool stage 1a: compositor premultiplied mode

Date: 2026-10-14. Lock `pool-stage1a` (pre-registered; `kill-locks.sh`
not called per the brief). MAIN tree. NO cargo, NO commits. Surface: the
3 locked files only — `crates/umber-gpu/src/compositor.rs`,
`crates/umber-app/tests/gate0_journey.rs` (struct literal), this report.

Read before writing: the design doc §4.3 + §8 + §7 row 1a,
`compositor.rs` in full, `shaders.rs:575-590` (the premultiplied dab
splat whose data shape the flag serves), `paint.rs` (`PaintTarget`:
usages, `write_region`, `texture()`), `layers.rs` (`BlendMode`
derives), and the wgpu 30.0.1 registry source for
`copy_texture_to_texture`.

## 1. Built

**`crates/umber-gpu/src/compositor.rs`** (only code file):

- `premultiplied: bool` on `CpuStackLayer` and `GpuStackLayer`.
  `from_layer` defaults it to `false` on both (the constructor the brief
  names; no `Default` impl — a derived `Default` would also zero
  `visible`/`opacity`, which is not a safe entry). Every pre-existing
  struct literal in the file (32 sites: helpers, oracle tests, GPU
  fixture) gains `premultiplied: false`; no expectation changed.
- The premultiplied composite path (doc §4.3 item 1, judgments 10):
  `unpremultiply_texel` decodes `s = p / a` for `a > 0` (`a == 0`
  zeroes the channels and `cov == 0` makes it a no-op), clamped to
  `0..=1` — which is exactly "clamp `p` to `a`"
  (`min(p,a)/a == min(p/a,1)` for `a > 0`); then
  `composite_pixel_masked` runs unchanged. WGSL mirrors it behind
  `flags` bit 0.
- The bottom-copy identity rule (doc §4.3 item 2, judgment 11):
  `is_bottom_copy` fires when no entry has contributed yet and the
  first contributing entry is premultiplied + `Normal` +
  `clamped_opacity == 1.0` + unmasked. CPU seeds `byte/255` per texel
  (exact round-trip through `f32_to_u8`); GPU seeds by
  `copy_texture_to_texture` of the entry's tile overlap — exact by
  construction. "Contributed" means passing the skip filters (visible,
  pixel-bearing, non-`Passthrough`); even a transparent bottom entry
  counts, so the seed fires at most once per call, at the true bottom.
- `composite_stack_region` + `composite_stack_region_cpu` + public
  `CompositeRect { x, y, width, height }` (doc §4.3 item 3, judgment
  12). The region path clears only the rect (transparent
  `write_region` — the retouch path's proven shape), folds the same
  entries with the same rule, uploads only the rect's mask bytes into a
  rect-sized `R8Unorm` texture, and never stores outside the rect. The
  CPU oracle returns the rect's bytes; region bytes equal the full
  composite's same rect EXACTLY (shared per-texel fold). Rect edges
  clamp to the tile; a fully-outside rect is a no-op (empty bytes /
  `Ok(())`).
- WGSL mechanism (the doc leaves it to the implementer): one shared
  kernel, no second variant. `CompositeParams` keeps its 32-byte layout
  with renamed tail words — `flags` (bit 0 = premultiplied; higher bits
  reserved), `origin_x`/`origin_y`. `width`/`height` carry the
  DISPATCH extent (output dims on the full path, rect extent on the
  region path), so the bounds check and the mask sample
  (`texel − origin`) read identically on both paths; full-path
  dispatches carry origin `(0,0)`, which is exactly the old guard and
  the old sample. Flag-off dispatches are arithmetic-identical to the
  old shader (same values, folded constants).
- Tests added: 9 (6 CPU + 3 GPU).
  - CPU §8 T1 `premultiplied_identity_…`: `(100,100,100,128)` masked →
    `[100,100,100,128]`; straight sentinel → `[50,50,50,128]`.
  - CPU §8 T2 `bottom_copy_seeds_verbatim`: `(200,200,200,100)` →
    verbatim; masked-path sentinel → `[100,100,100,100]`.
  - CPU `bottom_copy_yields_to_any_prior_contributor`: transparent
    bottom, `Multiply` mode, and `0.5` opacity suppressions →
    `[100,100,100,100]`, `[0,0,0,128]`, `[50,50,50,64]`
    (exact-binary-fraction derivations in comments).
  - CPU `from_layer_defaults_to_straight_alpha`: byte-identity anchor.
  - CPU §8 T3 `region_composite_equals_full_composite_subrect`: exact
    equality + outside-rect no-op + overhang clamp pins.
  - CPU `region_bottom_copy_seeds_rect_verbatim`.
  - GPU twins (±1 LSB, reason in comments): premultiplied identity,
    bottom-copy verbatim, region with sentinel fill (inside ≈ oracle,
    outside EXACTLY sentinel). Plus a `composite_region` fixture
    helper.
  - §8 T4 (flag-off byte-identity) holds by construction: no existing
    expectation edited.

**`crates/umber-app/tests/gate0_journey.rs`**: one line —
`premultiplied: false` on the leg-3 `GpuStackLayer` literal.

**This report**: `docs/loop/kill-reports/2026-10-14-pool-stage1a.md`.

## 2. Judgment calls

Every call below is also written into `compositor.rs` module docs
(judgments 10–12) or the item docs.

- **Seed by texture copy, not seed dispatch.** The doc offers both; the
  copy is byte-EXACT (no UNORM round-trip) and needs no new shader
  code — both textures are `Rgba8Unorm` `PaintTarget`s carrying
  `COPY_SRC | COPY_DST` (`paint.rs:275-279`), verified by reading, not
  assumed. The overlap clamps to both textures because
  `copy_texture_to_texture` panics on overrun (verified in the wgpu
  30.0.1 registry source); the clamp is load-bearing. Bit 1 of `flags`
  stays reserved.
- **One kernel for both scopes, extent-carrying `width`/`height`.**
  Rejected a second shader variant (duplicated blend arms the next mode
  census would have to touch twice) and a region-mode bit (unneeded:
  origin `(0,0)` unifies both paths). Renamed `pad0/1/2` →
  `flags` (bit 0 = premultiplied; higher bits reserved) /
  `origin_x` / `origin_y`.

> **Repair note (dragon, 2026-10-14, at landing).** The author
> session's write path truncated this report at this point — the file
> on disk ended mid-judgment-2 with a literal `...[truncated 3152
> chars]` marker (byte-verified). The sentence above was completed
> from `compositor.rs` module-doc judgment 10 ("the flag rides
> `CompositeParams.flags` bit 0 (the old `pad0`)") and judgment 12
> ("the rect origin in `origin_x`/`origin_y`"), which carry the same
> calls verbatim; whatever judgment prose the author wrote beyond it
> is unrecoverable and is NOT reconstructed here. Every load-bearing
> judgment call of this kill lives in full in `compositor.rs`'s
> module docs (judgments 10-12) and in §1's built/test list above,
> both of which are complete. §1 was verified against the live diff
> (1099-line compositor.rs change, 9 new tests, the one-line
> gate0_journey.rs literal); the battery receipt for the landing is
> the commit this note rides in.