# Kill report — wave-blends: the full pro blending set (2026-10-12)

## 1. Built

Files touched (2 source files + this report; requirements.md NOT edited per brief):

- `crates/umber-core/src/layers.rs` — the blend engine + mode enum:
  - 17 new `BlendMode` variants (10 separable + 7 triplet), census docs rewritten.
  - `blend_channel`: 10 new scalar arms; dodge/burn refactored into shared
    `color_dodge`/`color_burn` helpers (VividLight reuses them).
  - New: `blend_rgb` (triplet entry point), `is_triplet`, `is_normal_map`,
    `rgb_to_hsv`/`hsv_to_rgb`, `decode_normal`/`normalize_or`,
    `whiteout`, `rnm`.
  - Tests: 18 new/updated (see count below).
- `crates/umber-gpu/src/compositor.rs` — the compositor mirror:
  - WGSL: 17 new `BLEND_*` ids (14–30), 10 new scalar arms, `color_dodge`/
    `color_burn` helpers, `rgb_to_hsv`/`hsv_to_rgb`/`whiteout`/`rnm`/
    `blend_triplet`/`normal_encode_or_base`/`clamp01v`, kernel triplet branch.
  - Rust: `blend_mode_id` +17 arms, `composite_pixel` triplet routing,
    module-docs judgments 6/head updated.
  - Tests: ids extended, dyadic table +10 rows, naive-fold list +10 modes,
    3 new CPU tests, 3 new GPU goldens.

Test count: layers.rs +18 test fns (17 per-mode/oracle tests: divide,
inverse-divide, inverse-subtract, exclusion, addsub, vivid-light,
linear-light, pin-light, disable/replace, hsv-round-trip, tint, saturation,
color, value, combine, detail, inverse-detail; plus the new triplet census
pin — and the separable census test updated 14→24 in place). compositor.rs
+6 test fns (byte pins, dyadic extension in place, triplet RGB pins,
triplet oracle-fold, 3 GPU family goldens) with 2 existing tables extended
in place (ids, naive fold). Total: 24 new test fns, 4 existing extended.

## 2. Census before/after

BEFORE (15): Normal, Passthrough, Multiply, Screen, Overlay, Darken,
Lighten, Add (= Linear Dodge), Subtract, Difference, SoftLight, HardLight,
ColorDodge, ColorBurn, LinearBurn.

AFTER (32 = Painter's documented list, verified 2026-10-12 against the live
blending-modes doc page): the 15 above plus Divide, InverseDivide,
InverseSubtract, Exclusion, AddSub, VividLight, LinearLight, PinLight,
Disable, Replace (separable) and Tint, Saturation, Color, Value,
NormalCombine, NormalDetail, NormalInverseDetail (triplet).

Census arithmetic: 15 built + 10 separable + 7 triplet = 32. The remaining
~17 of the brief are exactly these 17. Painter's Dissolve is the one
documented mode with no variant (noise-based, needs per-texel RNG/seed
plumbing — deliberately excluded, recorded on the enum docs).

## 3. Math per family (all in linear space, src = top, dst = base)

- Divide `d/s` (s≤0→white); InverseDivide `s/d` (d≤0→white); InverseSubtract
  `max(0,s−d)`; Exclusion `s+d−2sd`; AddSub `d+s−0.5` clamped.
- VividLight: burn(d,2s) for s≤0.5 else dodge(d,2s−1) (no-op at s=0.5,
  guards carried over). LinearLight: `d+2s−1` clamped (no-op at 0.5).
  PinLight: `min(d,2s)` / `max(d,2(s−0.5))` (no-op at 0.5).
- Disable ≡ dst (exact at any opacity — a dispatch skip spelled as a pixel
  op). Replace ≡ src at the primitive level (see §5: opacity-bypass delta).
- HSV (Tint/Saturation/Color/Value): HSV-cylinder component swaps —
  Tint (hₛ,s_d,v_d) with achromatic-src→dst guard; Saturation
  (h_d,sₛ,v_d), no guard (black src actively grays, red→white pins it);
  Color (hₛ,sₛ,v_d) with the Tint guard; Value (h_d,s_d,vₛ), no guard.
- Normal-map: Combine = Whiteout `normalize(n1.xy+n2.xy, n1.z·n2.z)` (Oat
  2007); Detail = RNM `t·(t·u)−u·t_z`, `t=base+(0,0,1)`,
  `u=(−dx,−dy,dz)`, renormalized (Barré-Brisebois & Hill, "Blending in
  Detail", 2012, HLSL Appendix form with its recommended normalize kept);
  InverseDetail = RNM roles exchanged. Flat is identity on both sides for
  all three; degenerate (zero-length) decodes fall back to base.
- CORRECTION TO THE BRIEF (verified against both sources above): the brief
  guessed Combine=RNM / Detail=strength-blend and Reflect/Glow. The doc
  page says Combine=Whiteout, Detail=RNM ("more precise than Combine"),
  InverseDetail=roles-exchanged RNM — implemented as documented. Reflect/
  Glow are not in Painter's list and stay out.

## 4. Working-space decision

HSV conversion runs directly on the linear working triplet — no sRGB
round-trip. Grounds: Painter documents "all blending modes are performed
in linear gamma space internally" with the HSV ops as HSV-model ops, so
in-space conversion is the faithful reading; it also keeps the CPU oracle
and the WGSL mirror bit-agreeing instead of smuggling a transfer function
into one arm. Hue in linear ≠ hue in sRGB — accepted as the engine's
uniform contract (architecture.md: blending is linear per channel).
Recorded in `blend_rgb` docs. Near-black chromatic texels still take hue 0
(the doc's "very dark colors remain unchanged" is implemented as the
exactly-achromatic guard) — documented approximation on the Tint variant.

## 5. Judgment calls (also written into module docs)

1. Painter vocabulary wins over Photoshop: variants named Tint (not Hue)
   and Value (not Luminosity); doc comments record the aliases.
2. HSV-cylinder model, not Photoshop's setLum/setSat clip-color model
   (simpler, exactly pinnable, matches the doc's "performs the operation
   with the HSV model").
3. No DirectX/OpenGL Y-flip in the normal math — operates on raw encoded
   triplets; the flip belongs to import/export color management.
4. Normal-channel gate is a routing contract (`is_normal_map` predicate
   for the channel-routing/viewport-texture surface), not a math check —
   `ChannelKind` has no Normal kind (only Color/Data) and PoolKey encodes
   that 2-way split, so adding a kind would spill into tile-pool surfaces
   owned by other kills.
5. Replace v1 ≡ Normal at every level (opacity/alpha bypass deferred to
   the channel-routing work); Disable is already exact.
6. Unity's contemporary basis-transform normal blend (same article) is
   deliberately NOT used — its basis degenerates away from flat normals
   (the article's Figure 8); the quaternion RNM does not.
7. GPU arm: one golden per family (VividLight / Tint+Color / Detail+
   Combine), not per mode — all modes share one dispatch shape; CPU tests
   pin each arm's math. GPU-vs-CPU stays ±1 LSB.
8. Existing test names with stale counts (`all_fourteen_modes…`) left
   untouched; new tables added alongside (minimal churn).

## 6. Requirements delta (for the dragon to apply — requirements.md NOT edited)

Row §2 `Blending modes: full pro set (~32 …)` 4th cell proposal:

`| ✓ built (PENDING) — full 32 verified: 24 separable + Passthrough + 7 triplet (HSV Tint/Saturation/Color/Value in linear working space; normal Combine=Whiteout, Detail/InverseDetail=RNM per Painter docs + Barré-Brisebois & Hill 2012). v1 boundaries: Replace ≡ Normal (opacity-bypass deferred to channel routing); no normal Y-flip (import/export CM surface); Dissolve excluded (noise-based, not separable-safe); fill-layer path (umber-app composite_fill_over) degrades triplet modes to Normal — follow-up for the fill owner |`

Also closes the §2 `Per-channel blending mode + opacity; blending computed
in linear space per channel` row's "blend census 15" note → "blend census 32".

## 7. Unverified

- No compiler available in this harness (brief: NO cargo): all 24 new +
  4 extended tests are hand-verified reasoning-only (anchors re-derived
  twice; the one real bug found by re-reading — swapped VividLight
  dodge/burn args — is fixed; the tests as written would have caught it).
  Needs CI (ubuntu + windows) as the runtime gate.
- WGSL triplet fns never compiled here (naga 30 via CI/GPU tests only);
  `var h: f32` made explicit to dodge abstract-float inference risk; `%`
  avoided; no isNan/isFinite used (NaN-idiom comments mirrored).
- RNM/Whiteout formulas verified against the primary source (selfshadow
  article, Appendix code pasted verbatim-shape); Painter's mode list +
  linear-space + HSV-model + Combine=Whiteout/Detail=RNM claims verified
  against the live Experience League page 2026-10-12.
- Byte-row `(BLACK, AddSub, WHITE) → GRAY128` relies on 127.5→128
  half-away rounding (same as the existing opacity-ramp pin).

## 8. Deferred (all recorded in module docs)

- Replace opacity/alpha bypass; normal-channel routing enforcement;
  normal Y-convention flip; Dissolve; fill-layer triplet degradation
  (`composite_fill_over`'s `unwrap_or(s)` treats triplet None as Normal —
  umber-app surface, flagging for the fill owner, no silent panic today).
- Serde note: new unit variants are backward-compatible (old saves load;
  new names only appear in new saves). No UI/i18n strings added (no
  blend-mode dropdown exists in umber-app today; when one lands it needs
  `tr()` keys per house rule 7).

## 9. Gates run

- `scripts/verify-kill.sh`, `cargo fmt`, `cargo clippy -p umber-core /
  -p umber-gpu --all-targets -- -D warnings`: NOT RUN — brief hard
  constraint (NO bash scripts, NO cargo). Verbatim: not run, per task wall.
  Formatting kept to rustfmt shape by hand (or-pattern style, ≤100 cols
  outside WGSL string, inline format args, no `matches!` 3-arg/doc-blank
  violations); CI is the gate. Trap self-check done: no `source` field, no
  struct-in-impl, no format field access, no `map_or(true)`, no neg-cmp
  (`!len_sq.is_finite() || len_sq <= …` is a method negation, not a flipped
  comparison), E0507/E0596 patterns absent, single `mod` untouched.
