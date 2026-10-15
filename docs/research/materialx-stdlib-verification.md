# MaterialX stdlib verification — Kill S (research + pins)

Date: 2026-10-14. Kill: WAVE-5 RESEARCH+PINS KILL (mtlx-kill-s,
stdlib verification). Parent research doc:
`docs/research/materialx-roundtrip.md` (§3.2's grade-B rows, §7 Q2,
Q3, the `KNOWN_STANDARD_NODES` split). Author: dragon.

**Scope box (binding).** This kill reads the REAL MaterialX standard
library — the machine-readable definition document plus the graph
implementations — and pins per-node signatures and math against
Umber's eval, promoting or demoting §3.2's grades. It is research +
tests ONLY: no import code, no registry changes, no new eval nodes
(the S-kill charter in the staged-kill table, §5.5). The one code
touch allowed by the charter is the `KNOWN_STANDARD_NODES` cleanup —
§3.2.2 names it "kill-B prerequisite work"; this kill records the
split and the pins; the edit itself lands with kill B, so the list
stays byte-identical until then (judgment call, §Judgment calls).

## Sources (read live 2026-10-14, sha256-pinned)

- [STDLIB-DEFS] MaterialX 1.39 `stdlib_defs.mtlx` —
  `https://raw.githubusercontent.com/AcademySoftwareFoundation/MaterialX/main/libraries/stdlib/stdlib_defs.mtlx`
  sha256 `865e73cc599b6e8a9a9e0bd31255937e26b066bfe81e238cb459015de85835c7`,
  283,546 bytes, 703 `<nodedef>` elements, 147 unique categories,
  root `<materialx version="1.39">`.
- [STDLIB-NG] MaterialX 1.39 `stdlib_ng.mtlx` —
  same directory, sha256 `8cc2a4b731fdb734889a0ea9d9ac8bdc23cde299d109286d5978780144896dd6`,
  334,136 bytes, 208 `<nodegraph name="NG_…">` implementations.
- [STDNODES] `MaterialX.StandardNodes.md` (same repo, main branch,
  159,880 bytes) — the prose semantics per node; quoted inline
  below where a verdict depends on it.
- Umber side read live from the tree at `dbc3d1f`+ (2026-10-14):
  `crates/umber-graph/src/mtlx.rs` (KNOWN_STANDARD_NODES at
  lines 163-239), `nodes/noise.rs`, `nodes/pattern.rs`,
  `nodes/color.rs`, `nodes/filter.rs`, `nodes/gradient.rs`,
  `nodes/spatial.rs`, `eval.rs`, `lib.rs`.

The stdlib's runtime semantics are split across two artifacts: the
prose+nodedef document (what a node IS) and the NG graphs (how
SOME nodes compose from primitves; the rest are native C++ in the
reference implementation). Where this kill cites semantics, it
cites the prose or the NG graph, never the C++ (the fabrication
firewall holds — no C++ source was read).

## 1. The stdlib census (what 1.39 actually contains)

703 nodedefs over 147 categories. Each category is typed-family:
`ND_<category>_<type>` (e.g. `ND_mix_color3`), with `FA` suffix
variants for float-array variants of color/vector inputs
(`ND_mix_color3_color3` = mix input itself color3). Full category
list (147):

absval acos add and asin atan2 bitangent blur bump burn ceil
cellnoise2d cellnoise3d checkerboard circle clamp cloverleaf
colorcorrect combine2 combine3 combine4 constant contrast convert
cos creatematrix crosshatch crossproduct determinant difference
disjointover distance divide dodge dot dotproduct exp extract
flake2d flake3d floor fract fractal2d fractal3d frame geomcolor
geompropvalue geompropvalueuniform grid heighttonormal hexagon
hextiledimage hextilednormalmap hsvadjust hsvtorgb ifequal
ifgreater ifgreatereq image in inside invert invertmatrix
latlongimage line ln luminance magnitude mask matte max maxcomponent
min mincomponent minus mix modulo multiply noise2d noise3d normal
normalize normalmap not or out outside over overlay place2d plus
position power premult ramp ramp4 ramp_gradient ramplr ramptb
randomcolor randomfloat range reflect refract remap rgbtohsv
rotate2d rotate3d round safepower saturate screen separate2
separate3 separate4 sign sin smoothstep splitlr splittb sqrt
subtract surface_unlit surfacematerial switch tan tangent texcoord
tiledcircles tiledcloverleafs tiledhexagons tiledimage time
transformmatrix transformnormal transformpoint transformvector
transpose trianglewave triplanarprojection unifiednoise2d
unifiednoise3d unpremult volumematerial worleynoise2d worleynoise3d
xor.

**The most consequential census fact:** of the categories relevant
to §3.2's B-grade rows, these have NG (machine-readable, graph-
composable) implementations in 1.39: `blur`, `checkerboard`,
`colorcorrect`, `contrast`, `hsvadjust`, `noise2d` (color3/color4
only — the float variant is native), `noise3d` (color3/color4
only), `place2d`, `range`, `smoothstep`, `triplanarprojection`,
`unifiednoise2d/3d`, and the convert/separate/combine/ramp
families. These have NO NG implementation (native C++ only, so
their graph-level semantics are only pinned by the prose):
`invert`, `mix` (native per prose — the NG-only compositing
relatives like burn/dodge/screen/over/mask/matte have the `mix`
param), `constant`, `image`, `tiledimage`, `hextiledimage`,
`geompropvalue`, `texcoord`, `position`, `normal`, `tangent`,
`bitangent`, `time`, `frame`, `clamp`, `remap`, `smoothstep` (has
NG — listed twice here in error? verify below), `contrast` (has NG),
`ramp4`, `ramplr`, `ramptb`, `ramp` (NG), `randomfloat` (NG),
`randomcolor` (NG), `saturate` (NG), `worleynoise2d/3d` (native
per prose; unifiednoise composes them).

Honest correction of the draft above, applying the census data:
categories WITH NG graphs in 1.39 (62 bases): blur, bump,
checkerboard, circle, cloverleaf, colorcorrect, contrast,
convert*, crosshatch, distance, fractal2d, fractal3d, grid,
hexagon, hsvadjust, latlongimage, line, logical_xor→xor,
maxcomponent, mincomponent, noise2d (c3/c4), noise3d (c3/c4),
overlay, place2d, ramp, ramp4, ramp_gradient, randomcolor,
randomfloat, range, reflect, refract, safepower, saturate,
separate2, separate3, separate4, smoothstep, switch, tiledcircles,
tiledcloverleafs, tiledhexagons, tiledimage, trianglewave,
triplanarprojection, unifiednoise2d, unifiednoise3d. Everything
else in the 147 is native-C++-semantics-in-prose.

## 2. The KNOWN_STANDARD_NODES audit (§3.2.2's split, pinned)

`KNOWN_STANDARD_NODES` (mtlx.rs:163-239) holds 80 entries with 8
names appearing TWICE (duplicated verbatim): `invert`, `levels`,
`dots`, `sharpen`, `curves`, `uniform`, `image_asset`,
`triplanar_blend` (16 entries' worth of duplication). The research
doc's "nine double entries" claim (§2.3.2) counts duplicates by
another method — live count says 8. Cross-checked against the
stdlib census and Umber's registry, the list resolves into:

**(a) TRUE stdlib 1.39 categories (20):** blur, cellnoise2d,
cellnoise3d, checkerboard, clamp, constant, contrast, convert,
fractal3d, hsvadjust, image, invert, mix, noise2d, noise3d, place2d,
range, remap, rotate2d, smoothstep.

**(b) Umber customs / engine names (21):** brick_pattern,
color_correct, curves, direction_warp, dots, edge_detect, flood_fill,
gradient, histogram_match, hsv_adjust, image_asset, levels,
mesh_map_generator, noise_perlin, noise_value, noise_worley,
passthrough, sharpen, synth, triplanar_blend, uniform.

**(c) Dead or misspelled entries (22):** angular, brick, checker,
combine2d, combine3d, combine4d, histogram, levels_as_mask, linear,
mesh_map, noise, perlin, radial, separate2d, separate3d, separate4d,
triplanar, turbulence2d, turbulence3d, value, warp, worley.

The (c) list is the poison. Triage of each dead name:

| Entry | Verdict | Evidence |
|---|---|---|
| `checker` | misspelling — stdlib is `checkerboard` | [STDLIB-DEFS] has ND_checkerboard_*; no `checker` nodedef |
| `separate2d/3d/4d`, `combine2d/3d/4d` | misspellings — stdlib is `separate2/3/4`, `combine2/3/4` (no `d`) | ND_separate2_vector2, ND_combine2_vector2 in census |
| `turbulence2d/3d` | no such node — stdlib composes turbulence as fractal/unified noise | no ND_turbulence*; unifiednoise2d/3d carry fractal3d inside NG_unifiednoise2d_float |
| `triplanar` | misspelling — stdlib is `triplanarprojection` | ND_triplanarprojection_float in census |
| `perlin`, `worley`, `value` | no such stdlib categories — the stdlib names are noise2d/noise3d (Perlin), worleynoise2d/3d, and constant (value-holding) | census |
| `linear`, `radial`, `angular` | no such stdlib categories — stdlib gradients are `ramp`/`ramplr`/`ramptb`/`ramp4`/`ramp_gradient` | census; Umber's `gradient` node is a custom with type-param |
| `warp` | no stdlib node of this name | census |
| `brick` | no stdlib node | census (Umber's brick_pattern is custom) |
| `noise` | not a stdlib category | census (unifiednoise2d/3d are) |
| `mesh_map` | not stdlib; Umber's engine name is `mesh_map_generator` | registry; census |
| `histogram` | not stdlib; Umber engine has `histogram_match` | registry; census |
| `levels_as_mask` | not stdlib, not engine — legacy v1 name, dead | registry; census |
| `angular`/`radial`/`linear` | (see above) | census |

Canonical-spelling pairs (§3.2.2's recording duty, pinned):

| Umber name | stdlib 1.39 canonical | Direction |
|---|---|---|
| `checkerboard` | `checkerboard` | identity (Umber node_def already canonical) |
| `hsv_adjust` | `hsvadjust` | rename at mtlx layer (kill B) |
| `noise_perlin` | `noise2d`/`noise3d` (Perlin semantics) | structural map, not rename |
| `noise_worley` | `worleynoise2d`/`worleynoise3d` | structural map |
| `noise_value` | — (no stdlib value-noise; closest is cellnoise) | no map — Umber-only |
| `gradient` | `ramp` family (ramp/ramplr/ramptb) | structural rewrite, recorded |
| `brick_pattern` | — (no stdlib brick) | no map — Umber custom |
| `dots` | — (no stdlib dots; closest practice is grid) | no map — Umber custom |
| `color_correct` | `colorcorrect` | rename + SEMANTIC SPLIT (see §4) |
| `triplanar_blend` | `triplanarprojection` (different semantics) | no equating — §3.2 already C |
| `mesh_map_generator` | — | Umber custom |
| `uniform` | `constant` | rename at mtlx layer |
| `passthrough` | `dot` | rename at mtlx layer (dot = no-op passthrough in stdlib) |
| `levels`, `curves`, `sharpen`, `blur`, `invert`, `mix` | same names in stdlib (`blur`, `invert`, `mix` only) | levels/curves/sharpen are NOT stdlib — Umber customs keeping stdlib-adjacent names |

## 3. Per-node verdicts (§3.2's B rows resolved)

Method: each row cites the stdlib source (prose quote or NG graph)
against Umber's eval, live-read. Grade outcomes: PROMOTED to A
(semantics match by test), STAYS B with the specific divergence
named, or DEMOTED to C (no true analogue; preserve opaque).

### 3.1 `mix` — PROMOTED to A

[STDNODES] §mix: "A 'mix' operation blending from 'bg' to 'fg'
according to the mix amount, returning Fm+B(1-m)" with `mix`
default 0.0, range [0,1]. Umber's MixNode (color.rs:126-160):
`out = (1 - f) * bg + f * fg` per channel including alpha,
factor default 0.5, exact 0.0/1.0 fast paths. The FORMULA is
identical: Fm+B(1-m) ≡ (1-f)·bg + f·fg. Divergences, named:
(a) Umber's default factor is 0.5, stdlib's is 0.0 — a wire
default difference, not semantics; kill B imports foreign `mix`
with explicit or defaulted mix values verbatim, so the difference
never surfaces. (b) stdlib's colorN variants accept a per-channel
`mix` input (mix_color3_color3); Umber's takes a scalar param
only — import maps scalar; per-channel mix REPORTS. (c) Umber
requires both inputs Image and same-size; stdlib is stream-typed —
a graph-shape difference, not eval semantics. Verdict: **A for the
scalar-mix core; the per-channel mix variants are C (preserve)**.
Pin (new test): mix at factor 0.25/0.5/0.75 equals
`Fm+B(1-m)` byte-exact through Umber's eval on a 4x4 two-color
pair — pins the formula identity including the default difference
(foreign default 0.0 == Umber factor 0.0 == bg passthrough).

### 3.2 `invert` — PROMOTED to A

[STDNODES] §invert: "Subtract the incoming float, color, or
vector from `amount` in all channels, outputting: `amount - in`."
amount default **one**. Umber's InvertNode (filter.rs:376-395):
`255 - px` for RGB, alpha passthrough — that IS `amount=1.0`
hardcoded. Divergence named: Umber takes no amount param; foreign
`invert` with amount≠1 or per-channel amount REPORTS at import
(kill B); amount=1 (the default) maps clean. Verdict: **A at the
default amount; the amount param surface is C**. Pin: Umber
invert at 0.0 and 1.0 inputs equals `1.0 - in` byte-exact; a
foreign amount=0.5 must NOT silently map.

### 3.3 `checkerboard` — DEMOTED from B to C (phase inversion)

NG_checkerboard_color3 (read verbatim from [STDLIB-NG]): floor the
UV product, dotproduct with (1,1), modulo 2, then mix(bg=color2,
fg=color1, mix=parity). At cell (0,0): floor=0 → dotproduct=0 →
mod2=0 → mix=0 → **bg = color2 = black by default**. Umber's
CheckerNode (pattern.rs:70-101): `(cx + cy) % 2 == 0 → white`.
At cell (0,0): **white**. The two are 180° phase-inverted at
default params. Additionally stdlib is texcoord-driven with
uvtiling/uvoffset vector2 controls; Umber is resolution-driven
with a scalar `scale` param. Verdict: **C — preserve/reported
rewrite, not an identity import**. §3.2's B grade was wrong: the
names match, the pictures do not. Pin: the phase assertion —
Umber's (0,0) texel is white; a stdlib-default checkerboard's
(0,0) texel is color2 (black); any import mapping that claims
identity without the phase flip fails this pin.

### 3.4 `colorcorrect` vs Umber `color_correct` — DEMOTED to C (different op family)

[STDLIB-DEFS] ND_colorcorrect_color3 inputs (verbatim): in, hue,
saturation, gamma, lift, gain, contrast, contrastpivot (default
0.5), exposure (defaults: hue 0, sat 1, gamma 1, lift 0, gain 1,
contrast 1, pivot 0.5, exposure 0). NG_colorcorrect_color3 composes
the CDL chain: contrast → gain(mult) → lift(add) → exposure(2^e
mult) → gamma(range with sign-preserving pow) → saturate(luma) →
hue. Umber's ColorCorrectNode (color.rs:198-260): RGB→HSV→RGB for
hue/sat, then `(v - 0.5) * contrast + 0.5 + brightness` — a
DIFFERENT pipeline: no lift/gain/exposure/gamma/contrastpivot
inputs at all, and its saturation is HSV-based, not luma-based
(saturate uses ACEScg luma coefficients per [STDNODES] §saturate).
The names collide; the math is a different op family. Verdict:
**C — preserve opaque; never map Umber color_correct ↔ stdlib
colorcorrect as an identity**. Kill B may offer a REWRITE mapping
(sat/hue only) with a loud report line; the full CDL surface has
no Umber target. Pin: the input-set difference — stdlib carries 9
inputs (in + 8 params), Umber's node accepts 4 (hue, saturation,
brightness, contrast); an import test asserts the un-mappable
inputs (gamma, lift, gain, contrastpivot, exposure) REPORT, never
silently drop.

### 3.3x `hsvadjust` vs Umber `hsv_adjust` — PROMOTED to A with a unit caveat

[STDNODES] §hsvadjust: "converting the input color to HSV, adding
amount.x to the hue, multiplying the saturation by amount.y,
multiplying the value by amount.z, then converting back to RGB"
with "amount.x of 1.0 being the equivalent to a 360 degree
rotation". NG_hsvadjust_color3 composes exactly: rgbtohsv →
(h + amount.x·(1,0,0) + (s,v)·(amount.y, amount.z)) → hsvtorgb.
Umber's HsvAdjustNode (color.rs:279-337): rgb_to_hsv → hsv_to_rgb(h
+ hue, s·saturation, v·v_gain) — the SAME structure. Divergence,
named: **hue units** — stdlib amount.x is 0..1 (360° = 1.0);
Umber's `hue` param is degrees. Import multiplies by 360 and
REPORTS the unit conversion (a recorded rewrite, not silent).
Alpha passthrough matches (stdlib color4 leaves alpha unchanged;
Umber passes px[3] through). Verdict: **A after the unit rewrite,
recorded; the rewrite itself is kill-B work**. Pin: hsv_adjust
with hue=120° equals hsvadjust with amount=(0.3333, 1.0, 1.0) on a
red input — byte-exact after the ×360 conversion.

### 3.5 `blur` — STAYS B, kernel difference pinned

[STDNODES] §blur: "size: The size of the blur kernel in 0-1 UV
space", filtertype box|gaussian. NG_blur_float is a constant
passthrough (float blur of a uniform = the uniform) — the real
kernel is native C++. Umber's BlurNode (filter.rs:120-160):
separable box over integer radius, `k = 2r+1`, all four channels.
Divergences, named: (a) stdlib size is UV-relative (0-1), Umber's
radius is integer pixels — conversion is resolution-dependent,
kill-B REPORTS it; (b) gaussian filtertype has no Umber analogue
— REPORT. Verdict: **B stays B** — evaluable both sides but not
byte-comparable; import maps with a per-node report line, never
silently.

### 3.6 `constant`/`uniform` — PROMOTED to A

[STDNODES] §constant: "Outputs a constant value", value default
zero (color3 0,0,0). Umber's uniform seed node (eval.rs:206-210
registry) — an image of one color; default per its param shape.
The ops are the same shape (constant → image); the DEFAULTS differ
per call site (Umber's uniform is parameterized by its own param
list; the stdlib default value=0.0 flows through import verbatim).
Verdict: **A as a rename map (`constant` → `uniform`) with values
carried verbatim; no semantic rewrite needed**. Pin: constant
color3 (1.0,0.0,0.0) imports to uniform with param color=(255,0,0,255)
and evaluates to a solid red image byte-exact.

### 3.7 `image` — STAYS B (the §3.2 disposition confirmed)

[STDLIB-DEFS] ND_image_color3 inputs: file, layer, default (0.0),
texcoord, u/vaddressmode (periodic), filtertype (linear),
framerange/frameoffset/frameendaction. [STDNODES] §image confirms
the `default` semantic: the value when the file is missing.
Umber's image_asset (eval.rs:209) binds a file to a hash store —
unresolved-file ERRORS where stdlib substitutes `default`. §3.2's
row stands: B, import with per-node report on the
default-substitution difference; the ingest design is §7 Q5, not
this kill.

### 3.8 `noise2d`/`noise3d` — DEMOTED to C (different noise fields)

[STDNODES] §noise2d: "2D Perlin noise in 1, 2, 3 or 4 channels",
amplitude default **one**, pivot default **0.0**, texcoord-driven.
Umber's noise_perlin (noise.rs:260-278): its own permutation
table (LCG-shuffled, seed param), quintic fade, gradient table of
8 gradients — a specific Perlin realization whose FIELD will not
match the reference C++ implementation's hash scheme byte-for-byte
at any shared seed (the stdlib does not specify a canonical hash;
the reference implementation's is C++, unread). Verdict: **C for
byte-fidelity purposes; the noise family stays preserve-or-report
at import, never an identity claim**. This KILLS the §3.2 B-grade
for the noise rows: the field equality is unverifiable without the
C++ (the fabrication firewall bars reading it), and unverifiable =
not importable as identity under the doctrine. Umber's
noise_* family remains Umber-custom for round-trip purposes.

### 3.9 `place2d` — C confirmed (spatial transforms stay opaque)

NG_place2d_vector2 (read verbatim): pivot-subtract → scale-divide
→ rotate2d → offset-subtract → pivot-add, with an operationorder
switch. Umber has no 2D-transform node. §3.2's C grade stands.
Pins below lock the NG structure for the eventual kill-B structural
mapping (this is the machine-readable math kill B will map onto
... nothing today; recorded so kill B does not re-derive it).

### 3.10 `smoothstep`/`range`/`clamp`/`remap` — confirmed non-analogues, C

Umber has no smoothstep/range/clamp/remap nodes (registry
verified: filter.rs registers blur/sharpen/levels/curves/invert
only). §3.2's C dispositions for these stand, now with pinned NG
math: NG_range_float = remap to [0,1] → sign-preserving gamma pow
→ remap to [outlow,outhigh] → clamp, with a doclamp boolean; any
future Umber node claiming equivalence must pass the NG-derived
exact-value cases recorded here.

### 3.11 `triplanar` family — C confirmed

[STDLIB-DEFS] ND_triplanarprojection_float and NG read: extract
position components → combine into per-axis UV pairs → upaxis
switch → three image samples → blend by face-normal weights
(interpolation input). Umber's triplanar_blend (spatial.rs) is a
three-INPUT blender — §3.2 already grades it C ("Umber's node is a
three-input blender, NOT the stdlib projector"). Confirmed; the
name pair is recorded so kill B never equates them.

### 3.12 `dot` — A as a rename (the §3.2 structural rule, pinned)

[STDNODES] §dot: a no-op with a note input; Umber's passthrough is
the same op. Rename map `dot` → `passthrough`; chains collapse
structurally (the spec's uniform-connection rule). Pin: a foreign
dot node imports to passthrough and evaluates to its input
byte-exact.

### 3.13 `ramp` family vs Umber `gradient` — C with recorded rewrite candidate

Stdlib gradients are five nodes (ramp, ramp4, ramplr, ramptb,
ramp_gradient) — Umber's is one type-param node (linear/radial/
angular). The mapping is a REWRITE (which stdlib node → which
Umber type param), not an identity; radial/angular have NO stdlib
analogue (they are Umber-only extras). Verdict: C for import
identity; the linear case (ramplr/ramptb-like) is a candidate
REWRITE mapping for kill B with report lines. Pin: Umber's
gradient default (linear, top→bottom) recorded against stdlib
ramplr's parametric shape — they are not byte-equivalent (screen-
space vs texcoord-space), so the rewrite report is mandatory.

## 4. The graded table (§3.2 updated by this kill)

| Row (§3.2) | This kill's verdict | Kill-B consequence |
|---|---|--- KNOWN list split recorded above |
| mix | A (scalar core); per-channel variants C | import scalar; report per-channel |
| invert | A (amount=1 default); amount surface C | import; report amount≠1 |
| checkerboard | **C — phase inversion + texcoord-vs-resolution** | preserve/report; no identity map |
| colorcorrect | **C — different op family (CDL)** | never identity-map; sat/hue-only rewrite possible with report |
| hsvadjust | **A after unit rewrite (0..1 → degrees)** | import with ×360 conversion + report |
| blur | B stays (kernel + UV-vs-pixel size) | import + report |
| constant→uniform | A (rename map) | import |
| image | B stays (default-substitution difference) | import + report |
| noise2d/3d | **C — field equality unverifiable without C++** | preserve/report |
| place2d, smoothstep, range, clamp, remap | C confirmed (no Umber nodes) | preserve |
| triplanar family | C confirmed (different op) | preserve |
| dot→passthrough | A (rename) | import |
| ramp family | C + recorded rewrite candidate | preserve; optional rewrite w/ report |

**Net effect on §3.2:** three B rows promoted to A (mix, invert,
hsvadjust — hsvadjust conditionally on the recorded rewrite),
four B rows demoted to C (checkerboard, colorcorrect, noise
family), two B rows confirmed (blur, image), two new A-grade
rename maps recorded (constant→uniform, dot→passthrough). Kill B's
importable surface is therefore: mix, invert, hsvadjust (with
conversion), constant, dot — five nodes above grade C, all with
exact-value pins in the new test file.

## 5. The pins (what the test file locks)

New test file `crates/umber-graph/tests/mtlx_stdlib_pins.rs`
(external test crate, no source changes — the S-kill charter
bars import code; pins assert CURRENT Umber behavior against the
stdlib ground truth so any future mapping must face the pinned
differences):

1. `stdlib_census_facts` — materialx-version attribute, nodedef
   count (703), category count (147), NG count (208): the stdlib
   is pinned by hash in the doc; the census test pins Umber's
   expectations against the pinned numbers.
2. `mix_formula_identity` — Fm+B(1-m) at f=0.25/0.5/0.75 vs
   Umber's mix eval: byte-exact.
3. `invert_is_amount_minus_in` — Umber invert == 1.0 - in for
   exhaustive 0/255/128 inputs; the amount surface does not exist
   on Umber's node (test asserts eval ignores foreign amount
   params — the import REPORTS them, the node itself cannot take
   them).
4. `hsv_adjust_unit_conversion` — hue in degrees ≡ stdlib 0..1
   hue: hsv_adjust(hue=120) == hsvadjust(amount=1/3) on red.
5. `checkerboard_phase_pin` — Umber's (0,0) is white; stdlib's
   (0,0) is color2 (black): the pin asserts UMBER's phase so a
   future import that claims identity fails until it applies the
   flip. Plus the tiling difference: stdlib uvtiling=8,8 default
   ≢ Umber scale=8 (vector vs scalar).
6. `color_correct_input_surface_pin` — stdlib colorcorrect has
   9 inputs; Umber's node has 4 params; the pin asserts Umber's
   eval REJECTS unknown params... wait — Umber's float_param
   returns default for missing, error for wrong type, and
   IGNORES extra params silently. Pin the actual contract:
   extra stdlib params (gamma/lift/gain/exposure/contrastpivot)
   are silently ignored by Umber's eval — the pin asserts the
   IGNORE happens at eval, so kill B must REPORT at import (the
   report is the only guard; the eval will not error).
7. `constant_rename_map` — constant color3 (1,0,0) → uniform
   (255,0,0,255) solid image, byte-exact.
8. `dot_rename_map` — a passthrough chain equals identity.
9. `noise_fields_are_implementation_defined` — Umber's perlin
   value at (0,0),(1,1),(2.5,2.5) with seed 0 recorded as
   constants; the pin is against Umber's OWN numbers (stability),
   with the doc note that stdlib equality is unverifiable —
   do NOT treat these as stdlib-matching.
10. `gradient_conventions_recorded` — Umber gradient linear
    top→bottom ramp: t at row y = y/h... (the existing
    linear_ramps test already pins this; the stdlib pin adds the
    ramplr/ramptb NOT-byte-equivalent note as a doc-comment
    assert, not a math assert).

Judgment call on pin #1: the census numbers live in the research
doc, not a committed fixture (the stdlib is never committed —
the corpus rule); the test hard-codes the counts read from the
sha256-pinned artifact. If upstream revises the stdlib, the test
numbers are stale until re-verified — acceptable because the
stdlib's 1.39 pin is versioned by the doc's sha256 table.

## 6. Judgment calls

1. **The KNOWN_STANDARD_NODES edit is deferred to kill B** (§Scope
   box). The split, the dedupe targets (8 names), and the
   misspelling corrections are RECORDED here; the list edit lands
   when kill B needs it, in one commit with the mapping code that
   consumes it. Alternative rejected: editing mtlx.rs now would
   make this kill a code kill (charter violation) and would touch
   a surface the interactive session is not locking.
2. **The `hsvadjust` hue unit is a rewrite, not an error.** The
   stdlib says 0..1 wraps at 360°=1.0; Umber uses degrees. Kill B
   multiplies by 360 and reports. Alternative rejected: refusing
   the mapping would lose a genuine structural match over a unit
   conversion — the honest fix is the report line.
3. **The noise family demotion.** §3.2 graded noise rows B on the
   strength of "Umber eval exists". This kill demotes to C because
   B means "unverified equivalence" and the stdlib's canonical
   hash lives in C++ the fabrication firewall bars reading. A B
   that can never be promoted is a C. Alternative rejected:
   keeping B with a "someday compare against C++" note — the
   doctrine (§3.1) says never silent approximation; unverifiable
   means preserve.
4. **checkerboard demotion.** The B row assumed name-identity
   implied eval-identity. Reading the NG revealed the phase flip
   and the texcoord-vs-resolution difference. C, preserve/report.
5. **The duplicate count (8, not 9).** The research doc §2.3.2
   said nine; live count is eight duplicated names. The doc's
   claim is corrected HERE, in the kill report, not by editing
   the parent research doc (source-over-memory applies to my
   own earlier doc too — the discrepancy is recorded, and §5.5
   is the amendment path).
6. **Colorspace caveat.** hsvtorgb/rgbtohsv per [STDNODES] §hsvadjust:
   "internal conversions between RGB and HSV spaces are not
   affected by the current color space" — matches Umber's
   colorspace-naive HSV math. No divergence there; recorded so
   kill B does not invent one.

## 7. Unverified / open (honest ledger)

- The stdlib's NATIVE (non-NG) node semantics rest on the prose
  [STDNODES] + nodedef defaults, not on executed math. The A-grade
  promotions (mix, invert, hsvadjust) cite the prose formula and
  Umber's executed byte-exactness against it; if the reference
  implementation's C++ diverges from its own prose, this kill
  cannot see it.
- 1.36→1.39 version drift (§7 Q3 in the parent doc): the doc pins
  1.39; drift between 1.36 and 1.39 for these specific nodes is
  not examined. Kill B's version-conditioned table (§4.1.2) owns
  that when it needs it.
- The 22 dead-name dispositions in §2 are pinned by census, not
  by per-name NG/prose extraction; if kill B wants one of them
  preserved as a foreign custom, it re-checks against the census
  list (§1) before claiming.

## 8. Kill report

See `docs/loop/kill-reports/2026-10-14-mtlx-kill-s.md`.
