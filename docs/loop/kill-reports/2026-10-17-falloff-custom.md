# Kill report — falloff-custom (2026-10-17)

- Author: claw (subagent), dispatched by the dragon. HEAD `efd0cbf`.
- Lock `falloff-custom`: crates/umber-gpu/src/paint.rs, crates/umber-gpu/src/shaders.rs,
  crates/umber-brush/src/falloff.rs, crates/umber-brush/src/preset.rs,
  crates/umber-brush/src/dab_adapter.rs, crates/umber-app/src/brush_panel.rs, this report.
- **Touched beyond the six:** `assets/i18n/en-US.ftl` and `assets/i18n/pt-BR.ftl`. The brief
  explicitly ordered the label key in BOTH catalogs. Without it,
  `every_falloff_curve_has_a_distinct_catalog_key` goes RED, because it iterates
  `FalloffCurve::ALL`. Each catalog gains one line.
- Contract: docs/specs/falloff-custom-design.md.

## BLOCKERS: the dragon must apply these before the battery (outside the lock)

1. **E0063 in two full `BrushParams` literals.** Both list every field with no `..`, so the new
   `falloff_custom` field breaks them. This is the same trap the 2026-10-14 falloff kill hit.
   Fix each by adding one line:
   - `crates/umber-brush/src/preset_library.rs:266-276` (test `test_preset`): add
     `falloff_custom: Default::default(),`. This breaks `cargo test -p umber-brush`.
   - `crates/umber-paint/src/paint_state.rs:5734-5744` (a GPU test literal): add
     `falloff_custom: Default::default(),`. This breaks `cargo test -p umber-paint`, so
     **umber-paint must join the battery** (umber-gpu + umber-brush + umber-paint + umber-app).
2. **The LUT upload has no in-lock call site.**
   - `set_stamp` is reached only through `PaintState::set_stamp` (paint_state.rs:1887).
   - `PaintState`'s `compositor` and `queue` are private, with no accessor, and paint_state.rs
     is not in the lock. So nothing in the six files can reach
     `PaintCompositor::set_falloff_lut`.
   - I did NOT write a call to a method that doesn't exist.
   - **Until the hunk below lands, a Custom brush paints the 1x1 white-fallback full disc in
     the app.** The editor edits the preset, and the dabs carry id 10, but the drawn curve never
     reaches the GPU.
   - Proposed hunk for `crates/umber-paint/src/paint_state.rs`, inside `sync_brush_params`. It
     sits between strokes already, and main.rs:899 runs it every frame even with the panel
     hidden:

```rust
    pub fn sync_brush_params(&mut self, params: &BrushParams, preserve_color: bool) {
        if self.stroking {
            return;
        }
        let before = self.brush; // NEW (BrushParams is Copy)
        let color = self.brush.color;
        self.brush = BrushParams {
            wash_mode: self.brush.wash_mode,
            ..*params
        };
        if preserve_color {
            self.brush.color = color;
        }
        // NEW: the Custom falloff's LUT, uploaded only when the curve
        // (or the switch onto Custom) changed: 256 taps once, not per frame.
        if before.falloff != self.brush.falloff
            || before.falloff_custom != self.brush.falloff_custom
        {
            if let Some(lut) = self.brush.falloff_lut() {
                self.compositor.set_falloff_lut(&self.queue, lut);
            }
        }
    }
```

   The session starts on `BrushParams::default()` (Smooth), so a preset that starts on Custom
   uploads on its first sync. `falloff_lut()` deliberately ignores `stamped` (see its doc), so
   the stamped→unstamped toggle can't miss an upload. Once this lands,
   `PaintCompositor::set_falloff_lut` and `BrushParams::falloff_lut` have their production
   consumer (rule 6). Today their only consumers are tests.

## Built

- `crates/umber-gpu/src/paint.rs`:
  - New constants: `FALLOFF_CUSTOM = 10` and `FALLOFF_LUT_LEN = 256`. Ids 0-9 are untouched.
  - `PaintCompositor` gains:
    - `falloff_lut: RwLock<(Texture, TextureView)>` and `falloff_lut_needs_white: AtomicBool`,
      copying the stamp's shape exactly;
    - bind-group-layout entry 5 (a copy of entry 4, `Float { filterable: false }`, D2);
    - a 1x1 R32Float fallback created at construction, with its 1.0 texel recorded lazily on the
      first splat's encoder (a 256-byte row, `1.0f32.to_le_bytes()` in bytes 0..4);
    - `set_falloff_lut(&self, queue, samples: [f32; 256])`, which builds a fresh 256x1 R32Float
      texture each call (`TEXTURE_BINDING | COPY_DST`), uploads through `write_texture` with
      `bytemuck::cast_slice` and `bytes_per_row` 1024, and stores `needs_white = false`;
    - `falloff_lut_len()`, the width (1 = fallback);
    - the read guard and the white-texel copy in `splat_dabs`, plus bind-group entry 5.
  - 3 tests:
    - `shader_carries_the_custom_falloff_lut` (pure): the id and length constants, the
      `FALLOFF_CUSTOM: u32 = 10u` const, `case 10u`, the binding-5 declaration, and the exact
      tap expression.
    - GPU `a_custom_dab_without_a_lut_paints_the_full_disc`: fallback width 1. The centre and
      (44,32) read exactly `[255,0,0,255]`, and (50,32) reads `[0,0,0,0]`. Smooth at (44,32) is
      pinned to the CPU smoothstep at t = √156.5/16 (≈ 0.122, byte 31), ±1 for unorm rounding.
      That is the `sharp_falloff_dims…` precedent.
    - GPU `a_step_lut_custom_dab_paints_a_ring`: taps 0..128 = 0 and 128..256 = 1.
      - At h 0: the centre and (36,32) are `[0,0,0,0]`, (44,32) is `[0,255,0,255]`, and (50,32)
        is clear.
      - At h 0.75: the core texel (40,32) is clear (tap 0) and (46,32), tap 159, is painted.
      - The h-0.75 case also pins that the deferred white texel never overwrites a real LUT's
        tap 0. An h-0 golden alone would not catch that.
- `crates/umber-gpu/src/shaders.rs`:
  - `@group(0) @binding(5) var falloff_lut: texture_2d<f32>;` and `const FALLOFF_CUSTOM: u32 = 10u;`.
  - `radial_falloff` gains a `case 10u` arm:
    `let tap = min(u32(e * 255.0), textureDimensions(falloff_lut).x - 1u);` then
    `textureLoad(falloff_lut, vec2<i32>(i32(tap), 0), 0).r`, with the same `e` as the analytic
    arms. The `vec2<i32>` coordinates match every other `textureLoad` in this shader (stamp,
    mask). The doc comment names the CPU mirror.
- `crates/umber-brush/src/falloff.rs`:
  - `FalloffCurve::Custom` is a unit variant; the enum is still `Copy + Eq + Hash + serde(lowercase)`.
  - `ALL` grows to `[_; 10]` with Custom last. `as_u32` maps it to `gpu::FALLOFF_CUSTOM`.
    `from_u32(10)` returns `Some(Custom)` via `ALL`, and `from_u32(9)` and `from_u32(11)`
    return `None`.
  - **Shape: `eval_with(self, t, hardness, custom: &ControlCurve) -> f32`** is the full CPU
    reference. The existing `eval(self, t, hardness)` keeps its signature and delegates with
    `&ControlCurve::identity()`.
  - New `CUSTOM_LUT_LEN` (= `gpu::FALLOFF_LUT_LEN`), private `custom_index(e)` (the shader's
    truncated, clamped tap), `pub custom_tap(curve, i)`, and `pub custom_lut(curve) -> [f32; 256]`
    (`std::array::from_fn`). The LUT builder and `eval_with` share `custom_tap`.
  - Module docs gain a "`Custom`" section holding every judgment call below.
  - Tests: 1 updated, 4 new.
    - Updated `ids_are_stable_and_round_trip`: ids are `0..9 ++ [10]`, `from_u32` is checked at
      9, 10 and 11, `ALL[9] == Custom`, and `"custom"` on the wire.
    - New `custom_eval_is_the_luts_truncated_tap`: t 0.38/h 0 and t 0.69/h 0.5 both equal
      `curve.evaluate(1 - 96/255)`, about 0.3412; then `lut[96]`, a 256-tap parity loop, and the
      core and rim values.
    - New `custom_identity_is_linear_at_the_taps_and_eval_uses_it`.
    - New `analytic_curves_ignore_the_custom_curve`: bit-equal for all nine analytic curves.
    - New `custom_taps_clamp_out_of_range_curves`: y = 2 and y = -1 clamp to 1 and 0.
  - The existing `every_curve_is_one_at_centre_and_core_edge` and
    `every_curve_but_constant_is_zero_at_the_rim` now cover Custom too, through `ALL` with the
    identity default. Both pass only with the u-axis orientation.
- `crates/umber-brush/src/dab_adapter.rs`:
  - `BrushParams.falloff_custom: ControlCurve`, `#[serde(default)]`, set to identity in the
    manual `Default`. `BrushParams` stays `Copy`.
  - `BrushParams::falloff_lut() -> Option<[f32; 256]>` returns `Some` iff `falloff == Custom`.
    This is the pure half of "the apply path". It makes no GPU call, and the adapter stays
    GPU-object-free per its module contract.
  - `.with_falloff(params.falloff.as_u32())` is unchanged.
  - 2 tests:
    - `custom_falloff_round_trips_and_a_missing_curve_is_identity`: `"falloff":"custom"` with no
      `falloff_custom` loads as identity. The drawn curve serializes as
      `"falloff_custom":[[0.0,0.0],[0.25,0.8],[0.6,0.3],[1.0,1.0]]` and round-trips. Id 10 comes
      out of both the build-up and wash paths, and the LUT equals `custom_lut`.
    - `falloff_custom_without_custom_falloff_is_inert`: Sharp plus a drawn curve gives
      Debug-identical dabs to Sharp plus identity, and `falloff_lut() == None`. A stamped Custom
      brush keeps its LUT and still emits stamp-id dabs.
- `crates/umber-brush/src/preset.rs`:
  - The fixture preset carries a drawn `falloff_custom` (inert under Sphere), so the existing
    byte-identical round trip covers the field.
  - `validate()` re-checks `params.falloff_custom` alongside the pressure pair.
  - 1 test, `custom_falloff_files_round_trip_and_default_the_curve`:
    - `"falloff": "custom"` survives the file round trip;
    - with the key stripped, the file loads as identity and validates;
    - a non-monotone `falloff_custom` gives `NonMonotonicX`.
- `crates/umber-app/src/brush_panel.rs`:
  - The falloff combo row (now ~592-612): when Custom is selected, `falloff_custom_editor` adds
    `umber_gui::CurveEdit::new(..).size(120.0).fixed_count(true)` to the SAME
    `horizontal_wrapped` row, under `push_id("brush_falloff_custom_curve")`.
  - `falloff_key` gains `F::Custom => "brush.falloff-custom"`.
  - Pure helpers: `falloff_edit_model` (ControlCurve → `umber_gui::Curve`, sanitized) and
    `apply_falloff_edit` (writes back, returns whether the curve changed).
  - The `test_preset` literal gains `falloff_custom`.
  - Module docs gain the Custom paragraph.
  - 2 tests:
    - `falloff_custom_model_round_trips_and_writes_back`;
    - `falloff_custom_model_sanitizes_what_the_editor_refuses`, which covers the y clamp, the
      MIN_GAP nudge, the identity fallback, and `brush.falloff-custom` in both catalogs via
      `I18n::has_key`.
- `assets/i18n/en-US.ftl`: `.falloff-custom = Custom`. `assets/i18n/pt-BR.ftl`:
  `.falloff-custom = Personalizada` (feminine, to agree with *Atenuação*).

**Test count:** 12 new (3 umber-gpu, of which 2 are GPU-gated; 4 falloff.rs; 2 dab_adapter.rs;
1 preset.rs; 2 brush_panel.rs) plus 1 updated (`ids_are_stable_and_round_trip`).

## Judgment calls

1. **The editor's x axis is `u = 1 - e`** (0 = rim, 1 = core edge). Tap `i` = `curve(1 - i/255)`.
   - The design says only "sampled at t". Taken literally, `curve(e)` makes the default identity
     curve paint 0 at the centre and across the whole hardness core: a hollow ring as the
     *default*. It also fails the two existing ALL-iterating invariant tests.
   - With `u`, identity = Linear at the taps, and it matches the module's formula axis and
     Blender's custom-curve orientation.
   - The shader's index (`e`) is exactly the design's; the flip lives only in `custom_tap`.
2. **The read is a truncated nearest tap, not a lerp:** `min(floor(e*255), 255)` on both sides,
   with the same f32 ops. That gives the design's "exact parity, same rounding". It means a
   "piecewise-linear read" is the LUT of a piecewise-linear curve, not a lerp between taps.
3. **The shader clamps the tap to `textureDimensions(falloff_lut).x - 1u`,** not the design
   snippet's literal `clamp(.., 0u, 255u)`. Against the 1x1 fallback, the design's index would
   read out of bounds, which is implementation-defined under wgpu's robustness policy. On a
   256-wide LUT the two are identical.
   - The design snippet also passes a u32 into `vec2<i32>(..)`, which doesn't compile in WGSL.
     I used `vec2<i32>(i32(tap), 0)`, an explicit cast in the shader's existing coordinate
     type.
4. **The case is written `case 10u`, a literal,** so the existing `case {id}u` text pin and the
   new one both match. The WGSL `FALLOFF_CUSTOM` const is declared for the Rust-side pin but
   unused in code. An unused module-scope const is legal WGSL.
5. **The setter takes `queue: &wgpu::Queue`.** The brief's `set_falloff_lut(&self, samples)` has
   no queue, and the compositor holds none. `set_stamp` takes one too.
   - Samples are passed by value, `[f32; 256]`, per the brief.
   - Non-finite taps store 0.0; the rest clamp to 0..=1. The brush builder clamps as well,
     defensively.
   - No `clear_falloff_lut` exists: it would have no consumer (rule 6). Nothing ever needs to go
     back to the fallback, because the LUT is only read by Custom dabs.
6. **Store `needs_white = false` on set.** Otherwise the deferred white copy lands on tap 0 of
   the real LUT, which is the tap the hardness core reads. The h-0.75 golden pins this.
7. **The Custom core is flat at `curve(u=1)`,** not always 1.0, because `t <= h` means e = 0,
   which is tap 0. With the identity default that is 1.0. Documented in the module docs.
8. **Taps clamp to 0..=1.** `ControlCurve` only requires finite points; hand-edited JSON can
   carry y = 2.
9. **`eval` (two arguments) evaluates Custom with the identity curve.** It can't see the params,
   and I kept the signature. `eval_with` is the curve-aware sibling.
10. **The panel keeps the combo beside the editor (no literal swap).** A swap leaves no way back
    out of Custom. The design's own words are "the combo and the editor share the row".
11. **`CurveEdit::fixed_count(true)`.** Otherwise click-to-insert makes 5 points, which
    `ControlCurve::try_new` rejects.
12. **The panel model is rebuilt each frame from the preset,** clamped into [0,1]², with each x
    nudged to at least `MIN_GAP + 1e-6` past its left neighbour. A curve still refused shows as
    the identity.
    - The nudge exists because `Curve::move_point` parks a point exactly `MIN_GAP` from its
      neighbour, and the f32 difference can come out an ulp short of `MIN_GAP` in the next
      frame's `Curve::new`. Without the nudge, the editor would snap to identity mid-drag.
    - Cost: an edit writes the nudged x back (a drift of at most about 1e-6).
13. **`falloff_lut()` ignores `stamped`.** A change-detected upload keyed on falloff/curve would
    otherwise miss the stamped→unstamped toggle and paint the fallback.
14. **Serde back-compat (the design's VERIFY), answered from the derive:** a binary older than
    `Custom` FAILS to load a preset whose falloff is `"custom"` (unknown variant, so the whole
    preset errors; it doesn't default). Old binaries do tolerate the extra `falloff_custom` key,
    because `BrushParams` has no `deny_unknown_fields`.
15. **No requirements row is marked.** docs/specs/requirements.md has no dedicated Custom-falloff
    row. Rows 15 and 154 mention falloff curves and the combo as built; neither names Custom.

## Blast-radius sweep (rung 4): every exhaustive site over `FalloffCurve`

- `falloff.rs`:
  - `ALL`: array, length 9→10, Custom appended.
  - `as_u32`: exhaustive match, arm added.
  - `eval_with`: exhaustive match, arm added. `eval` delegates.
  - `from_u32`: iterates `ALL`, no match.
- `brush_panel.rs`:
  - `falloff_key`: exhaustive match, arm added.
  - The combo iterates `ALL`, no match.
  - The test `every_falloff_curve_has_a_distinct_catalog_key` iterates `ALL`; covered by the new
    ftl keys.
- Serde: derived (`rename_all = "lowercase"`). No manual impls exist.
- `dab_adapter.rs::dab_for` uses `as_u32()`, with no match.
- `paint_state.rs:5740/5748`: a literal value and an equality only, no match.
- The repo-wide grep of `FalloffCurve::`, `FalloffCurve as` and `ALL[` finds no other match, and
  no positional `ALL` use outside the new test.
- `PAINT_COMPUTE_SHADER` / `radial_falloff(` are embedded only by paint.rs.
  - `umber-brush/src/retouch.rs` has its own Smooth-only `radial_falloff(t, h)`, which is not a
    `FalloffCurve` switch.
  - tile_pool, paint_thread and bake_shaders mention the shader in docs only. Custom dabs keep
    the radial footprint, so the footprint math there is unchanged.
- **Literals broken by the new `BrushParams` field:** see Blocker 1.
  - In-lock literals were fixed: brush_panel.rs `test_preset` and the preset.rs fixture.
  - The dab_adapter.rs literals all use `..`.
  - paint_state.rs:1869/8988 use `..`.

## Unverified

**Verification ladder:**
- Rungs 1 and 3 did NOT run.
  - Every Bash invocation of `~/.cargo/bin/rustfmt`, `bash scripts/verify-kill.sh`, `python3`,
    and even `git diff` and multi-file `grep` came back "requires approval" in this
    non-interactive session.
  - Formatting was done by hand against rustfmt's default heuristics: fn-call/chain width 60,
    max 100, single-line if-else ≤ 50. **Run `bash scripts/verify-kill.sh` on the eight files;
    it rewrites any fmt drift in place.**
  - Rung 3 (the Python f32 mutation port) did not run either. The hand-derived mutation table
    below takes its place:
    - At t 0.38: f32 0.38 × 255 = 96.8999… gives tap 96. Rounding gives 97 (zigzag ≈ 0.3343 vs
      0.3412), the unflipped axis gives x 0.3765 (≈ 0.6193), and a lerp gives ≈ 0.3350. So
      `custom_eval_is_the_luts_truncated_tap` fails under each of the three mutations.
    - Flipping the orientation also fails `every_curve_is_one_at_centre` (centre would read 0).
    - Dropping `needs_white = false` fails the h-0.75 golden (core texel painted).
    - Dropping the y clamp fails `custom_taps_clamp_out_of_range_curves`.
- Rung 2 (APIs) was resolved against in-repo precedent, not the registry. `~/.cargo/registry` is
  outside this session's readable directories.
  - `docs/claw-artifacts/umber-gpu/README.md` (rule 4's first source) was read. It covers the
    compute pass, pipeline and bind-group shapes for wgpu 30.0.1, but says nothing about
    R32Float, `write_texture` or `textureLoad`, so it neither confirms nor contradicts them.
  - wgpu: `write_texture`, `copy_buffer_to_texture`, `TexelCopy*`, `Texture::size()`,
    `BindingType::Texture { Float { filterable: false } }` are the exact shapes paint.rs's stamp
    already uses. `TextureFormat::R32Float` + `TEXTURE_BINDING` follows postfx_exec.rs:765.
  - egui: `Ui::push_id(..).inner` follows postfx_panel.rs:349.
  - umber-gui: `Curve`/`CurveEdit` (new/size/fixed_count/show/points/move_point/Default) were
    read from curve.rs:57-411. `MIN_GAP` is `pub` in `umber_gui::widgets::curve`, and `Curve`
    and `CurveEdit` are re-exported at the crate root (lib.rs:40).
  - WGSL `textureDimensions(texture_2d<f32>) -> vec2<u32>` comes from the WGSL spec, not checked
    against naga's source. The shader already relies on it (`stamp_sample` wraps it in
    `vec2<i32>(..)`).
- **GPU goldens unrun** (no cargo): `a_custom_dab_without_a_lut_paints_the_full_disc` and
  `a_step_lut_custom_dab_paints_a_ring`. Both are under `--features gpu`, with the
  `try_request_device` graceful skip.
- **CPU↔GPU parity is exact only up to `e` itself.** WGSL division and `distance` aren't
  correctly rounded, so a texel whose `e*255` sits within a few ULPs of an integer can land one
  tap apart on the GPU. The goldens keep probes ≥ 30 taps from the step for that reason. The
  design's test-plan item 4 (WGSL `radial_falloff(0.7, 0, 10)` vs CPU) is not built: there is no
  existing WGSL-evaluating parity harness in paint.rs to extend, only text pins and the
  one-texel Sharp golden.
- Whether any other path (umber-cli batch, the agent JSONL session) writes `PaintState.brush`
  without `sync_brush_params` was not audited. Such a path would skip the Blocker-2 upload too.
- Old-binary behavior (judgment call 14) comes from serde-derive semantics, not from a run.

## Deferred

- The PaintState upload wiring and the two literal fixes: Blocker 1 and 2 above (outside the
  lock).
- Design test-plan item 4 (WGSL-evaluated parity at t 0.7). It needs a shader-eval harness that
  doesn't exist yet.
- The Levels editor's `CurveEdit` consumer (`GpuOp::Levels`): the design names it as the next
  kill (graph_eval.rs).
- R16Float LUT and other LUT sizes (design open questions 1-2): not taken. R32Float and 256 are
  the design's recommendation.

## Gates run

- verify-kill: **NOT RUN** (Bash "requires approval").
- rustfmt --check: **NOT RUN** (same). Formatted by hand.
- clippy / cargo: not run, as the brief forbids.
- Python mutation port: **NOT RUN** (same). The hand derivation is above.
