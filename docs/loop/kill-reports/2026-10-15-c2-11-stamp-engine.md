# Kill report: C2-11 bitmap stamps + dab angle, the engine half

Date: 2026-10-15. Author: dragon. Lock: `c2-11-stamp-engine` over
`crates/umber-gpu/src/{paint,shaders}.rs`, `crates/umber-gpu/src/lib.rs`
and this report. Source: audit C2-11 ("bitmap stamps → angle → jitter
quartet; stamp textures in the dab shader first") and ROADMAP
`wave-bitmap-stamps`, the prerequisite of angle / rake / random rotation.

## Built

- **`FALLOFF_STAMP = 9`**, a new falloff id. A stamp dab's coverage is
  the compositor's stamp texture sampled over the dab's SQUARE (side =
  2·radius), not a radial curve. `hardness` is ignored for it.
- **`Dab::with_angle(radians)`** rotates the stamp. The angle occupies
  the last padding slot (byte 44), so `Dab` stays 48 bytes and a default
  dab is byte-identical to before. A test pins bytes 40..48 as zero for
  the default and the f32 at 44 for a turned dab. Non-finite input
  becomes 0.
- **The shader:**
  - `stamp_tex` at binding 4 (R8, any size), read only by id 9;
  - `stamp_sample` is a manual bilinear lookup clamped to the edge, so
    the pass needs no sampler binding;
  - a stamp dab maps each texel into the stamp frame (rotate by −angle,
    scale the square to [0,1]²), and its bounding box grows to radius·√2
    so the corners of a rotated stamp are never clipped;
  - radial dabs take exactly the old path.
- **`PaintCompositor::set_stamp(queue, w, h, values)`**: uploads one R8
  stamp. It refuses (`InvalidRegion`, stamp unchanged) a zero side, a
  side over the device's 2D texture limit, or a byte count that doesn't
  match. `clear_stamp` returns to the fallback, and `stamp_size`
  reports what is set.
- **The fallback is a 1x1 white texel**, so a stamp dab with no stamp
  paints its full square. Construction has no queue, so the white texel
  is recorded on the first splat's encoder through an `AtomicBool` flag.
  The compositor is `Send + Sync`, which an existing test pins and the
  paint thread pool needs; the first draft's `Cell` failed that test.

## Tests (GPU, behind `--features gpu`, the CI lavapipe leg's flag)

- `a_stamp_dab_without_a_stamp_paints_its_full_square`: the square's
  corner (outside the r=16 disc) is painted, a texel outside the square
  is clear, and the same dab as a radial disc leaves that corner clear.
- `a_stamp_shapes_the_dab_and_the_angle_rotates_it`:
  - an 8×8 left-half stamp paints the left half and not the right;
  - a quarter turn makes left and right equal and top and bottom differ;
  - bad uploads are refused and the stamp is unchanged;
  - after `clear_stamp`, the deferred white upload re-arms and the full
    square paints.
- Default-feature tests: the angle byte slot, NaN → 0, and the shader's
  `angle` field and `FALLOFF_STAMP` constant in sync with Rust.
- The `paint::` gpu module ran 15/15 with `--features gpu`.
- Gate 1 (default features): umber-gpu 145, umber-paint 161, umber-app
  492, all green.
- **Mutation check (ran):** with the shader ignoring the angle, the
  rotation test fails at "rotated: left/right are no longer split".
- **The full `umber-gpu --features gpu` suite does not finish on the
  build host's NVIDIA GPU, and that predates this kill.** The same hang
  reproduces at the pushed base `4359c4d` without these changes:
  compositor + graph_eval GPU tests run past 60 s at 4 threads. It is the
  known driver deadlock class (see the `umber-wave-ops` skill: GPU suites
  on real NVIDIA hardware). This kill's own module (`paint::`, 15 tests)
  passes alone at 4 threads in under 2 s. The full gpu-feature leg is
  CI's lavapipe step (`ci.yml:95`), which this wave's CI run covers.
- Clippy `--features gpu --all-targets` reports two pre-existing
  `manual_range_contains` errors in `compositor.rs:3245`, a test under
  the gpu feature that the default battery never compiles. Not this
  kill's surface; named for the CI leg.

## Not done (the brush half, next kill)

- `BrushParams` gains a stamp (a PNG path or embedded R8) and an angle
  (fixed, follow-the-stroke, random). The dab adapter emits
  `FALLOFF_STAMP` dabs with `with_angle`, and the Brush panel picks the
  stamp. The jitter quartet (size / angle / position / flow) rides on
  the same dab fields.
- Stamps per preset: `set_stamp` is compositor-global today, so a stroke
  that changes the stamp must upload between strokes (documented on
  `set_stamp`). A per-dab stamp index into an array texture is the
  multi-stamp follow-up.
