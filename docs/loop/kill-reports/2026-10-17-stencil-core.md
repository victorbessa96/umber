# Kill report: stencil-core (wave-stencil, engine half — sequencing step 1)

- Contract: `docs/specs/stencil-engine-design.md` "The design" + "The
  test plan" item 1. HEAD `f84d625`.
- Kill-lock `stencil-core`: `crates/umber-core/src/stencil.rs` (new),
  `crates/umber-core/src/lib.rs` (one line: `pub mod stencil;`,
  between `smart` and `tiles`; no `pub use`).

## Built

`crates/umber-core/src/stencil.rs` (new, pure: no GPU, egui or I/O):

- `StencilTransform { center: [f32; 2], scale: f32, rotation: f32 }`.
  It derives Debug, Clone, Copy, PartialEq, Serialize and Deserialize,
  with a container-level `#[serde(default)]` and a manual `Default`
  (center 0,0 / scale 1 / rotation 0).
- `StencilImage` (private `width`, `height`, `coverage`, with
  accessors) plus `StencilImage::from_alpha(w, h, rgba) ->
  Result<_, StencilError>`. The rule is RGBA8 in, R8 coverage out:
  alpha >= 128 gives 255, anything lower gives 0.
- `StencilError` (thiserror, following `AssetError`'s style), three
  variants:
  - `EmptyImage { width, height }`
  - `TooLarge { width, height }`: `w*h*4` overflows `usize`
    (`checked_mul`)
  - `ByteCount { expected, actual }`
- `forward_map(transform, image_size: [u32; 2], image_point) -> [f32; 2]`
  (pub).
- `inverse_map(transform, image_size: [u32; 2], screen_point) -> [f32; 2]`.
- `rasterize_gate(image, transform, target_w, target_h) -> Vec<u8>`,
  `target_w * target_h` bytes, row-major, 255/0. This is the same
  byte shape `PaintTarget::write_mask_region` takes (umber-gpu
  paint.rs:570-596).
- `fit_to_viewport(image_w: u32, image_h: u32, view_w: f32, view_h: f32)
  -> StencilTransform`.

Tests: **14** inline `#[cfg(test)]` tests.

1. `default_transform_is_origin_unit_unrotated`
2. `serde_missing_fields_fill_from_default_not_zero`
3. `forward_and_inverse_round_trip_within_1e6`: 4 transforms × 64
   interior points, both directions.
4. `forward_map_places_the_image_center_and_turns_clockwise`
5. `identity_transform_reproduces_the_image_bytes`: a 4x4 asymmetric
   pattern.
6. `two_x_scale_doubles_each_texel_and_centers`: a 10x10 target, the
   doubled image inside a one-texel zero ring, pinned in full.
7. `quarter_turn_permutes_as_pinned`: the permutation is derived by
   hand in the comment, `out[y][x] = img[3-x][y]`.
8. `texels_outside_the_coverage_are_zero`: 6x6 around a full 4x4,
   plus a far-off center.
9. `a_sample_exactly_midway_opens_the_inclusive_threshold`: exactly
   127.5 opens and 63.75 stays blocked.
10. `a_degenerate_scale_blocks_the_whole_gate`: scale 0, -1, NaN and
    ∞, a NaN center, and a zero-size target.
11. `from_alpha_cuts_at_exactly_128`: 127 gives 0 and 128 gives 255.
    The RGB bytes are set to 200 so that a wrong-channel read would
    fail.
12. `from_alpha_rejects_empty_and_miscounted_input`: zero w, zero h,
    15 bytes, and RGB-sized input.
13. `fit_to_viewport_takes_the_limiting_axis_and_centers`: a 400x100
    image in a 200x300 viewport gives scale 0.5 and center (100,150),
    and the left edge touches x=0. A small image scales up (10x20 in
    100x100 gives scale 5).
14. `fit_to_viewport_degenerate_input_falls_back_to_unit_scale`

## Judgment calls

1. **The forward map's shape** (module docs): `screen = center +
   R(rot)·(image_px − image_size/2)·scale`, with `R(θ) = [[cos, −sin],
   [sin, cos]]`. Screen space is y-down, so a **positive rotation is
   CLOCKWISE on screen**, which test 4 pins. The forward map is a
   **pub fn** (`forward_map`), not a test helper, because the overlay
   quad draw in the umber-app kill needs the same corner math the gate
   uses.
2. **Serde: done, container-level `#[serde(default)]`, not
   field-level.** umber-core already depends on serde, so it isn't
   skipped. The brief said "serde(default) on fields". A field-level
   default fills a missing `scale` with `f32::default()`, which is 0,
   and that is a degenerate transform. The container form fills gaps
   from `Default`, which gives scale 1. Test 2 pins this, and it fails
   under the field-level form.
3. **The image dims are an argument** (`image_size: [u32; 2]`) to
   `forward_map`/`inverse_map`, not a transform field, as the brief
   asked. The parameter order is (transform, image_size, point).
4. **v1 boundary: target texel space is the transform's screen space.**
   `rasterize_gate` maps target texel centers straight through the
   transform. Any viewport-px → target-texel conversion is the shell's
   job.
   - ⚠ This is a **design gap for the next kill to settle**: the paint
     target is the UV-space texture, while the stencil is a
     screen-space quad. The design's "rasterize at target resolution"
     only lines up if the gate is consumed in screen space, or if the
     shell composes the screen→UV projection itself. The brief
     specified exactly this API, so I built it and am flagging the gap
     here rather than resolving it.
5. **Texel centers sit at `i + 0.5`** in both spaces. Bilinear uses
   `u = p − 0.5`, `floor`, and fractional weights.
6. **Out-of-bounds = zero border**: each bilinear tap outside the
   image reads 0, which is how I read the brief's "out-of-bounds
   sample = 0". With binary coverage and the inclusive 0.5 threshold,
   the stencil's edge falls exactly on the image bounds: a sample at
   image px x=0 or x=width opens, and anything further out stays shut.
7. **Inclusive threshold** (`>= 127.5`), matching the design's "alpha
   >= 0.5". Binary in v1; soft edges are the design's Open question 3.
8. **Map math in f64, API in f32.** Targets reach 8K px, where f32's
   ulp is about 5e-4, and the round trip is pinned at 1e-6. The trig
   is hoisted once per rasterize (a private `Mapping`).
9. **Never-crash guards** (umber-core denies unwrap/expect/panic):
   - A non-finite or non-positive scale returns an all-zero gate.
   - A NaN/∞ center or rotation goes NaN, falls outside the
     `in_reach` window, and reads 0.
   - The `in_reach` window (one texel past the bounds) also keeps the
     float→i64 casts and `x0 + 1` far from saturation.
   - Coverage reads go through `.get()`.
   - `width * height * 4` uses `checked_mul`, and the target length
     uses `saturating_mul`.
10. **`StencilImage` fields are private** (with accessors), so the
    invariant `coverage.len() == width * height` set up by `from_alpha`
    can't be broken from outside. This departs from the brief's
    literal `StencilImage { width, height, coverage }`; the struct has
    the same shape behind getters.
11. **Reset semantics conflict (named, not resolved):** the brief asks
    for "the largest scale keeping the image inside", which can scale
    above 1, and that is what's built (test 13 pins the upscale). The
    design's chord section words Reset as "fit-center, scale 1". The
    shell kill should choose: keep this, or clamp with `.min(1.0)` at
    the call site. Recorded in `fit_to_viewport`'s doc.
12. Degenerate `fit_to_viewport` input (zero image dimension, or a
    non-positive/non-finite view) returns scale 1, with the center at
    the sanitized view center (0 for an unusable axis).
13. **No requirements-row tick (rule 9):** this kill is the engine
    half only. The g6-stencil-overlay row closes with the shell kills.

## Unverified

- **NOTHING WAS COMPILED OR RUN.** No cargo, per the brief. The dragon's
  battery (`wave-battery.sh umber-core`) is the first compile.
- **Ladder rung 1 (rustfmt --check): NOT RUN.** `~/.cargo/bin/rustfmt`
  was refused (needs approval in this non-interactive session), and so
  were `verify-kill.sh`, `awk` and `grep` line-width probes. Instead I
  hand-formatted against rustfmt's defaults (no rustfmt.toml in the
  repo; the workspace edition is 2021): max_width 100, fn_call /
  chain / array width 60, struct_lit_width 18, trailing commas on
  vertical struct literals. Borderline constructs were rewritten into
  short statements so the layout isn't ambiguous. The longest line is
  `fit_to_viewport`'s signature at 98 chars (hand-counted). **A
  rustfmt diff is still possible; run `cargo fmt --all --check`
  first.**
- **Rung 1, partial: `wc -L stencil.rs` = 98**, so no line exceeds
  max_width 100. That was the only formatting probe that was allowed.
- **Ladder rung 2 (std API resolution): RUN, by grep against real
  in-repo usage.** No new dependencies; serde_json is already a
  regular umber-core dependency.

  | API in stencil.rs | Sibling precedent |
  |---|---|
  | `(w as usize).checked_mul(h as usize).and_then(\|n\| n.checked_mul(4))` | umber-wasm/src/wire.rs:105-106, umber-plugin-sdk/src/lib.rs:259-260, umber-bake/src/margin.rs:249-250 (same shape) |
  | `rgba.chunks_exact(4).map(\|px\| …px[i]…).collect()` | umber-paint/src/persistence.rs:249, umber-bake/src/ao.rs:483 |
  | `f.sin_cos()` → `(sin, cos)` order | umber-brush/src/line_constraint.rs:167, umber-gui/src/widgets/param_group.rs:115 |
  | `usize::saturating_mul` | stylus/src/xi2.rs:159, spp/src/hdf5.rs:1358 |
  | float `(a..=b).contains(&v)` | umber-core/src/layers.rs:2993 |
  | integer `!(a..=b).contains(&v)` | umber-core/src/project.rs:383 |
  | float `assert_eq!` in tests | umber-core/src/layers.rs:1952 |
  | `f64::from(u32)`, `f64::from(f32)`, `i64::from(u32)`, `f64::floor` | no grep run; std-stable lossless `From` impls |
- **Ladder rung 3 (Python port + mutation): NOT EXECUTED.** `python3`,
  including stdin heredocs, needed approval and was refused, and
  numpy wasn't reachable either. Below is a **hand-traced** mutation
  table: each mutant is applied mentally to the Rust (the same logic
  the port would carry) and traced through the pinned tests. Treat it
  as reasoning, not evidence. The port should be run when Python is
  available.

| # | Mutation | Killed by | Trace |
|---|---|---|---|
| M1 | threshold `>=` → `>` | test 9 | the midway sample is exactly 127.5, so it gives 0 where 255 is pinned |
| M2 | drop the target `+0.5` (sample at `x`, not `x+.5`) | test 5 | texel (0,0) averages 3 border taps + img[0][0], giving 0.25 → 0 where 255 is pinned |
| M3 | drop the sample's `−0.5` (u = p) | tests 5, 8 | texel (3,3) gives 0.25·img[3][3] → 0 where 255 is pinned |
| M4 | inverse rotates by `+θ` | tests 7, 3 | row 0 becomes `0 0 0 1` where `0 0 1 1` is pinned |
| M5 | inverse multiplies by scale instead of dividing | tests 6, 3 | p = (x+.5−5)·2+2, so the doubled block is wrong |
| M6 | inverse forgets `+image_size/2` | test 5 | p shifts by −2, and rows 0-1 read border |
| M7 | zero border → clamp-to-edge | test 8 | texel (4,0) at p=4.5 clamps to col 3, giving 255 where 0 is pinned |
| M8 | `ry` sign flipped (`cos·dy + sin·dx`) | test 7 | gives the transpose, row 0 = `1 1 0 0` |
| M9 | bilinear weights swapped (`fx` ↔ `1−fx`) | test 5 | fx=0 puts all weight on x0+1, a one-texel shift |
| M10 | degenerate-scale guard removed | test 10 | scale −1 is a 180° flip of a full image, so all 255; scale ∞ sends p to the image center, so 255 |
| M11 | `from_alpha` cut `>= 128` → `> 128` or `>= 127` | test 11 | 128 gives 0, or 127 gives 255 |
| M12 | `from_alpha` reads `px[0]` (R) | test 11 | RGB is 200, so all 255 |
| M13 | fit uses `max` instead of `min` | test 13 | scale 3 where 0.5 is pinned |
| M14 | field-level serde default / Default scale 0 | tests 2, 1 | scale 0 where 1 is pinned |
| M15 | `in_reach` window narrowed `[−1, w+1]` → `[0, w]` | **survives: equivalent mutant** | a sample in [−0.5, 0) has coverage < 0.5 and stays shut anyway; the window exists to guard the casts, not to change behavior |

- **The Python port: WRITTEN, NOT EXECUTED.** Stdlib only, f32
  emulated through `struct`. The internal map runs in Python floats,
  matching the Rust f64 `Mapping`. One command for the dragon:
  `python3 - <<'EOF'` … `EOF` with the block below. It prints the
  baseline (all 14 must pass, plus the measured round-trip worst case)
  and then killed/SURVIVED per mutant. Expected: every mutant killed
  except M15 (equivalent).

```python
import math, struct
def f32(v): return struct.unpack('<f', struct.pack('<f', v))[0]
M = None  # the active mutant

def div(a, b):  # IEEE division (Rust f64 semantics, no ZeroDivisionError)
    if b == 0.0:
        return math.nan if a == 0.0 or math.isnan(a) else math.copysign(math.inf, a) * math.copysign(1.0, b)
    return a / b

def default():
    return dict(center=(0.0, 0.0), scale=0.0 if M == 'M14' else 1.0, rotation=0.0)

def mapping(t, size):
    r = f32(t['rotation'])
    return dict(c=(f32(t['center'][0]), f32(t['center'][1])), s=f32(t['scale']),
                cos=math.cos(r), sin=math.sin(r), half=(size[0] * 0.5, size[1] * 0.5))

def forward(m, ix, iy):
    dx = (ix - m['half'][0]) * m['s']; dy = (iy - m['half'][1]) * m['s']
    return (m['c'][0] + m['cos'] * dx - m['sin'] * dy, m['c'][1] + m['sin'] * dx + m['cos'] * dy)

def inverse(m, sx, sy):
    dx = sx - m['c'][0]; dy = sy - m['c'][1]
    sn = -m['sin'] if M == 'M4' else m['sin']
    rx = m['cos'] * dx + sn * dy
    ry = (m['cos'] * dy + sn * dx) if M == 'M8' else (m['cos'] * dy - sn * dx)
    x, y = (rx * m['s'], ry * m['s']) if M == 'M5' else (div(rx, m['s']), div(ry, m['s']))
    if M != 'M6':
        x += m['half'][0]; y += m['half'][1]
    return x, y

def forward_map(t, size, p):
    x, y = forward(mapping(t, size), f32(p[0]), f32(p[1])); return (f32(x), f32(y))

def inverse_map(t, size, s):
    x, y = inverse(mapping(t, size), f32(s[0]), f32(s[1])); return (f32(x), f32(y))

def texel(img, x, y):
    w, h, cov = img
    if M == 'M7':
        x = min(max(x, 0), w - 1); y = min(max(y, 0), h - 1)
    if not (0 <= x < w and 0 <= y < h): return 0.0
    return float(cov[y * w + x])

def sample(img, px, py):
    w, h, _ = img
    lo, hx, hy = (0.0, w, h) if M == 'M15' else (-1.0, w + 1.0, h + 1.0)
    if not (lo <= px <= hx and lo <= py <= hy): return 0.0  # NaN fails too
    off = 0.0 if M == 'M3' else 0.5
    ux, uy = px - off, py - off
    x0, y0 = math.floor(ux), math.floor(uy)
    fx, fy = ux - x0, uy - y0
    if M == 'M9': fx, fy = 1 - fx, 1 - fy
    return (texel(img, x0, y0) * (1 - fx) * (1 - fy) + texel(img, x0 + 1, y0) * fx * (1 - fy)
            + texel(img, x0, y0 + 1) * (1 - fx) * fy + texel(img, x0 + 1, y0 + 1) * fx * fy)

def rasterize(img, t, tw, th):
    s = f32(t['scale'])
    if M != 'M10' and (not math.isfinite(s) or s <= 0.0): return [0] * (tw * th)
    m = mapping(t, (img[0], img[1])); off = 0.0 if M == 'M2' else 0.5
    out = []
    for y in range(th):
        for x in range(tw):
            v = sample(img, *inverse(m, x + off, y + off))
            out.append(255 if ((v > 127.5) if M == 'M1' else (v >= 127.5)) else 0)
    return out

def from_alpha(w, h, rgba):
    if w == 0 or h == 0: return ('EmptyImage', w, h)
    if len(rgba) != w * h * 4: return ('ByteCount', w * h * 4, len(rgba))
    ch = 0 if M == 'M12' else 3
    cut = {'M11a': lambda a: a > 128, 'M11b': lambda a: a >= 127}.get(M, lambda a: a >= 128)
    return (w, h, [255 if cut(rgba[i * 4 + ch]) else 0 for i in range(w * h)])

def fit(iw, ih, vw, vh):
    u = lambda v: v if math.isfinite(v) and v > 0 else 0.0
    vw, vh = u(f32(vw)), u(f32(vh)); pick = max if M == 'M13' else min
    sc = pick(f32(vw / iw), f32(vh / ih)) if (iw > 0 and ih > 0 and vw > 0 and vh > 0) else 1.0
    return dict(center=(f32(vw * 0.5), f32(vh * 0.5)), scale=sc, rotation=0.0)

PAT = [[1, 0, 0, 0], [1, 1, 0, 0], [0, 1, 1, 0], [0, 0, 0, 1]]
FULL = [[1] * 4] * 4
def img4(rows): return from_alpha(4, 4, [v for r in rows for on in r for v in (0, 0, 0, on * 255)])
def B(rows): return [on * 255 for r in rows for on in r]
ID = dict(center=(2.0, 2.0), scale=1.0, rotation=0.0)
QT = f32(math.pi / 2)
WORST = [0.0]

def t01_02(): d = default(); return d['center'] == (0.0, 0.0) and d['scale'] == 1.0 and d['rotation'] == 0.0
def t03():
    worst = 0.0
    for c, s, r in [((2.5, 1.5), 1.0, 0.0), ((2.5, 1.5), 2.0, 0.3), ((-1.25, 3.0), 0.75, -1.2), ((2.0, 2.0), 1.5, QT)]:
        t = dict(center=c, scale=s, rotation=r)
        for j in range(8):
            for i in range(8):
                p = (f32(0.25 + i * 0.5), f32(0.25 + j * 0.5))
                sp = forward_map(t, (4, 4), p); back = inverse_map(t, (4, 4), sp); again = forward_map(t, (4, 4), back)
                worst = max(worst, abs(back[0] - p[0]), abs(back[1] - p[1]), abs(again[0] - sp[0]), abs(again[1] - sp[1]))
    WORST[0] = worst; return worst <= 1e-6
def t04():
    t = dict(center=(10.0, 20.0), scale=2.0, rotation=QT)
    r = forward_map(t, (4, 4), (3.0, 2.0))
    return forward_map(t, (4, 4), (2.0, 2.0)) == (10.0, 20.0) and abs(r[0] - 10) <= 1e-6 and abs(r[1] - 22) <= 1e-6
def t05(): i = img4(PAT); g = rasterize(i, ID, 4, 4); return g == B(PAT) and g == i[2]
def t06():
    e = B([[0]*10, [0,1,1,0,0,0,0,0,0,0], [0,1,1,0,0,0,0,0,0,0], [0,1,1,1,1,0,0,0,0,0], [0,1,1,1,1,0,0,0,0,0],
           [0,0,0,1,1,1,1,0,0,0], [0,0,0,1,1,1,1,0,0,0], [0,0,0,0,0,0,0,1,1,0], [0,0,0,0,0,0,0,1,1,0], [0]*10])
    return rasterize(img4(PAT), dict(center=(5.0, 5.0), scale=2.0, rotation=0.0), 10, 10) == e
def t07(): return rasterize(img4(PAT), dict(ID, rotation=QT), 4, 4) == B([[0,0,1,1], [0,1,1,0], [0,1,0,0], [1,0,0,0]])
def t08():
    e = B([[1,1,1,1,0,0]] * 4 + [[0] * 6] * 2); i = img4(FULL)
    return rasterize(i, ID, 6, 6) == e and rasterize(i, dict(ID, center=(1000.0, -1000.0)), 6, 6) == [0] * 36
def t09():
    i = from_alpha(2, 1, [0, 0, 0, 255, 0, 0, 0, 0]); t = dict(center=(0.5, 0.5), scale=1.0, rotation=0.0)
    return rasterize(i, t, 1, 1) == [255] and rasterize(i, dict(t, center=(0.25, 0.5)), 1, 1) == [0]
def t10():
    i = img4(FULL)
    ok = all(rasterize(i, dict(ID, scale=s), 4, 3) == [0] * 12 for s in (0.0, -1.0, math.nan, math.inf))
    return ok and rasterize(i, dict(ID, center=(math.nan, 2.0)), 4, 4) == [0] * 16 and rasterize(i, ID, 0, 7) == []
def t11(): return from_alpha(2, 2, [v for a in (127, 128, 0, 255) for v in (200, 200, 200, a)]) == (2, 2, [0, 255, 0, 255])
def t12():
    return (from_alpha(0, 4, []) == ('EmptyImage', 0, 4) and from_alpha(3, 0, []) == ('EmptyImage', 3, 0)
            and from_alpha(2, 2, [0] * 15) == ('ByteCount', 16, 15) and from_alpha(2, 2, [0] * 12) == ('ByteCount', 16, 12))
def t13():
    t = fit(400, 100, 200.0, 300.0)
    return (t == dict(center=(100.0, 150.0), scale=0.5, rotation=0.0)
            and forward_map(t, (400, 100), (0.0, 0.0)) == (0.0, 125.0) and fit(10, 20, 100.0, 100.0)['scale'] == 5.0)
def t14():
    a = fit(0, 10, 100.0, 50.0); b = fit(10, 10, 0.0, math.nan)
    return a['scale'] == 1.0 and a['center'] == (50.0, 25.0) and b['scale'] == 1.0 and b['center'] == (0.0, 0.0)

TESTS = [t01_02, t03, t04, t05, t06, t07, t08, t09, t10, t11, t12, t13, t14]
def run(): return [f.__name__ for f in TESTS if not f()]
base = run(); print('baseline failures:', base or 'none', '| round-trip worst:', WORST[0])
for m in ['M1','M2','M3','M4','M5','M6','M7','M8','M9','M10','M11a','M11b','M12','M13','M14','M15']:
    M = m; fails = run(); M = None
    print(f"{m:5} {'killed by ' + ','.join(fails) if fails else 'SURVIVED'}")
```

  The port merges Rust tests 1 and 2 into `t01_02`, because serde
  can't be ported; M14 is modeled through `Default`. Python's
  `math.cos(inf)` raises, but no case feeds an infinite rotation.
- **The 1e-6 round-trip margin is hand-derived:** the f32 ulp is
  ≤ 4.8e-7 at |screen| < 8 and ≤ 2.4e-7 at |image px| < 4, and the
  worst stacked rounding is about 4.8e-7. This was never measured.
- **The 90° case relies on f32 `FRAC_PI_2`'s cos ≈ −4.4e-8** keeping
  samples within ~1e-7 of the texel centers, so the dominant weight is
  about 1. That's reasoned, not measured.
- **Rung 4 (blast radius): RUN.** Grepping `crates/**/*.rs` for
  `mod stencil|stencil::|StencilTransform|rasterize_gate|fit_to_viewport`
  hits only `umber-core/src/lib.rs` and `stencil.rs`. The other
  "stencil" hits in umber-gpu/umber-app are wgpu depth-stencil, which
  is unrelated.
- **Clippy: not run.** By reading: no `manual_range_contains` (range
  `.contains` is used), no `needless_range_loop`, no `source` field, no
  derivable Default (scale 1 is tuned), no `>7`-argument fns, and no
  `nonminimal_bool` (`!(in_x && in_y)`).

## Deferred

- Soft-edge coverage (design Open question 3): binary v1, a
  rasterize-only change later. Recorded in the module docs.
- The screen↔UV composition gap (judgment call 4): for the umber-app
  `stencil.rs` kill to settle. If it becomes a per-point gate (the
  shell feeds each target texel's projected screen position), the
  private `Mapping::inverse` + `StencilImage::sample` split already
  supports it: one small pub fn over those two, with no change to the
  math.
- Reset's scale-1 vs fit-scale conflict (judgment call 11): for the
  shell kill.
- The chord → transform mapping, `StencilState`, PNG load, gate
  upload/AND-composition, overlay draw and i18n are design sequencing
  steps 2-3 (umber-app), outside this surface.

## Gates run

- `~/.cargo/bin/rustfmt --check --edition 2021 …/stencil.rs`: **NOT
  RUN**. The tool returned, verbatim, `This command requires approval`.
- `bash scripts/verify-kill.sh stencil.rs lib.rs`: **NOT RUN**, refused
  with `This command requires approval`.
- `python3 - <<'EOF'` (the port), `awk`/`grep '.\{101,\}'` width
  probes: **NOT RUN**, each refused with `This command requires
  approval`.
- The Python port is **written** above, and the mutation table is
  hand-traced.
- `wc -L stencil.rs`: **RUN**, result 98 (≤ 100).
- Rung-2 API precedent grep: **RUN** (the table above).
- cargo fmt / clippy / test: **NOT RUN** (forbidden by the brief; the
  host is down).
- Blast-radius grep: **RUN, PASS** (only the two locked files reference
  the module).

## Dragon consumption record (2026-10-17, this loop session)

- Host DOWN since ~18:50 (the second freeze in 26h — the MCE pattern
  from docs/loop/build-host.md). The battery is parked; the kill
  commits when the host answers and Gate 1 runs green.
- Ladder rung 1 EXECUTED by the dragon: rustfmt --check clean on
  stencil.rs + lib.rs (claw's was hand-formatted; zero drift).
- Ladder rung 3 EXECUTED by the dragon: the report's Python port ran
  (exit 0): baseline zero failures, round-trip worst 2.38e-7 (within
  the 1e-6 bound), mutants M1-M14 killed by the pinned cases, M15
  survived exactly as predicted (output-neutral by construction).
- verify-kill: braces 105/105 + 24/24, fmt clean, trap-scans clean,
  both files under the stencil-core lock.
- State: STATICALLY VERIFIED, IN FLIGHT — battery + commit pending
  the build host's return (SSH watcher live, 30-min window).


## Cross-review addendum (2026-10-17 20:3x, the 18:55 loop session)

The report's Unverified section said the approval gate refused every
rung. All run now, host-free:

- rustfmt `--check --edition 2021` on stencil.rs + lib.rs: **PASS**
  (the hand-formatting held; `wc -L` 98 confirmed).
- The Python port (written in the report, never run) was INDEPENDENTLY
  REIMPLEMENTED and run against the five contract cases: identity ==
  own bytes, quarter-turn permutation `out[y][x] = img[3-x][y]`, 2x
  scale ring/inner, 1e-6 round-trip, exactly-127.5 opens. **ALL
  PASS** — with one honest note: my first port run failed 4 of 5
  cases because I missed the texel-center convention
  (`sample`'s `-0.5` before floor, `GATE_THRESHOLD = 127.5` inclusive)
  — the exact trap the claw navigated correctly. The convention is
  the design's own ("bilinear-sample the coverage at that point" with
  centers at i+0.5), and the claw's module docs state it explicitly.
- Mutation rung RUN (not hand-traced): M1 drop-the-half-texel-offset
  caught by identity; M2 rotation-sign flip caught by the quarter-turn
  pin; M3 strict-> at threshold caught by the 127.5 case. **PASS**.

**Verdict: stencil-core's math is verified correct. The only gates
still owed are the compile+test battery (host-gated — the host went
down at ~18:5x) and the serde-derive claim (umber-core's Cargo.toml
does not appear to carry serde — the claw's own API-precedent table
will answer whether the derives compile; if not, that is a one-line
fix the battery will name).**

## Serde-claim closure + drift re-verification (2026-10-17 23:5x, the 23:18 wake session e78fd809)

The cross-review verdict's one open caveat — "umber-core's Cargo.toml
does not appear to carry serde" — is RESOLVED host-free, no fix owed:
`crates/umber-core/Cargo.toml:12-13` carries `serde = { workspace =
true }` and `serde_json = { workspace = true }`, and the workspace
`Cargo.toml:52` pins `serde = { version = "1", features = ["derive"] }`
— so stencil.rs's `use serde::{Deserialize, Serialize}` (:47),
`#[serde(default)]` (:95) and the serde_json round-trip tests (:383-396)
all resolve against pinned workspace deps. The battery will not name a
serde fix.

Also re-verified this wakeup (independent reproduction + drift check):
rustfmt `--check --edition 2021` (rustfmt 1.9.0-stable) CLEAN on all
six parked wave files; `scripts/verify-kill.sh` over the six: braces
PASS (main.rs's 876/877 raw count is the documented string-literal
false-positive — 740/740 code-only, pre-existing at HEAD per the app
report's addendum), trap-greps clean, doc-attachment clean, lock-audit
PASS (every dirty file matches its live kill-lock); the 52239d
sibling's recovery manifest
(`.dragon/stencil-wave-recovery-manifest.txt`) re-hashed: **all 14
paths MATCH — pinned bytes == live bytes, zero drift.** Remaining
gates owed to the host: unchanged (compile + clippy + test battery).
Static verification only; does not upgrade suite-verified status.
