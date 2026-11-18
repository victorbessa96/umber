# Kill report: D2 cage contract alignment (2026-11-18)

Kill lock: `razul-d2-cage-contract`. Brief: `docs/loop/briefs/2026-11-18-d2-cage-contract.md`
(commit `b7052754`, docs-first) + corrected DoD line in `docs/loop/briefs/2026-11-16-d2-auto-cage.md`.
Resolves next-steps B1 and the D2 findings of `docs/loop/reviews/2026-11-17-d2-d5-cross-review.md`.

## Contract decision (D2 MED #1)

The 11-16 brief's ray recipe and its DoD line 72 contradicted each other; the
implementation follows the recipe. Verified against the shader
(`TRANSFER_BAKE_SHADER` line 1571: `origin = low_pos + n*front_offset + cage_off`,
and the app passes `front_offset = front` with or without a cage). Resolved
contract, now written into the brief: cage offset = `hit − low_pos` (miss →
`front * normal`); on flat geometry Inflate's offset is 0, so its ray origin
coincides with the no-cage v1 origin → byte-equal transfer output to auto-cage-off;
Manual returns `front * normal` (shipped D2 behavior). No implementation behavior
changed. The test is strengthened, not weakened: per-method offset assertions kept,
plus a new assertion that Inflate-on-flat reproduces the v1 (no-cage) ray origin.

## Changes

- `crates/umber-mesh/src/bake_support.rs`: extended
  `cage_from_high_low_inflate_matches_manual_on_flat` (v1-origin equality assertion);
  added named test `transfer_shader_cage_lerp_matches_cpu` (D2 MED #3) —
  `Cage::lerp_at` bit-exact (`==` per f32 component) against the manual
  `a*w + b*u + c*v` expansion mirroring WGSL `lerp3_cage`, plus degenerate-tri
  never-panics checks.
- `crates/umber-bake/src/transfer.rs`: extracted the inline cage vec4 packing into
  pure `pack_cage_vec4` (D2 MED #2) + unit test
  `pack_cage_vec4_is_16_byte_aligned_with_zero_padding` (xyz preserved, w = 0,
  byte length % 16 == 0).
- `crates/umber-app/src/bakes_panel.rs` + `assets/i18n/en-US.ftl` + `pt-BR.ftl`
  (D2 LOW): cage-method ComboBox now has a tooltip via `combo.response.on_hover_text`
  (egui 0.36 ComboBox has no builder `on_hover_text`), new `bakes.cage-method-hint`
  key in both catalogs, shortened to satisfy the ≤80-char inline-hint lint.

## Battery (flock /tmp/umber-cargo.lock, CARGO_BUILD_JOBS=3, nice 19)

- rustfmt clean on the 3 touched .rs; check-layers GREEN (16); scorecard current.
- clippy `-p umber-mesh -p umber-bake -p umber-app --all-targets -- -D warnings`: 0 errors.
- `cargo test -p umber-mesh -p umber-bake -p umber-app`: 20 suites, all ok
  (incl. the 3 target tests above).
- safe-push green; ls-remote verified; kill lock released; queue row 538.
