# Kill report: channel-export-resample (W-C3a lane A1)

- Authorship: claw `opencode/deepseek-v4.1-flash-free`.
- Kill-lock `channel-export-resample`, files:
  `crates/umber-app/src/bake_sources.rs`,
  `crates/umber-app/src/export_dialog.rs`,
  `docs/loop/kill-reports/2026-10-19-channel-export-resample.md`.
- Design source: `docs/loop/next-wave-plan-draft.md` §1 lane A1 (verified
  anchors) + `docs/loop/briefs/2026-10-19-channel-export-resample.md`.
- Closes: a set painted at one size and exported at another silently
  dropped ALL painted pixels. `painted_channel_tile` and
  `painted_base_color_tile` returned `None` on any dims mismatch, so a
  4K-painted set exported through the dialog's 2K preset wrote flat/baked
  bytes with no notice. Both readbacks now FIT the readback to the export
  size instead of declining.

## Built

- `crates/umber-app/src/bake_sources.rs`
  - New pure fn `fit_channel_to_export(bytes, (w, h), size, channel) ->
    Vec<u8>` (pub): identity `(w, h) == (size, size)` returns the caller's
    bytes untouched (byte-exact, no re-encode/round-trip); otherwise a
    bilinear fit through `umber_paint::material_layer::resample_image_rgba8`
    (texel-centre aligned, edge-clamped) with `channel` riding through so
    the `"normal"` arm renormalizes. Module + fn docs name the aliasing
    boundary and the box-filter deferral.
  - `painted_base_color_tile`: reads the tile target, then
    `Some(fit_channel_to_export(bytes, dims, size, "baseColor"))`. No dims
    gate; `None` only for an absent session/tile or a failed readback.
  - `painted_channel_tile`: same shape via `channel_target`, passing the
    channel name through. No dims gate; only an absent channel is `None`.
  - Module doc: new "Export-size fit (2026-10-19)" paragraph.
  - Tests (5): 4 new pure + 1 GPU re-pin (below).
- `crates/umber-app/src/export_dialog.rs`
  - `painted_at_size` becomes "a session exists":
    `Option<(u32, u32)> -> bool`, true iff `Some`. The `size` argument is
    dropped with the decline it gated; the name is kept for path
    stability. Doc rewritten.
  - Caller in `ExportDialog::show` updated to `painted_at_size(paint_size)`.
  - `prospective_base_source` doc updated (no longer size-gated);
    module doc gains the "Export-size fit (2026-10-19)" paragraph.
  - Badge comment names the now-unreachable `Some(paint_size)` arm (kept —
    see Judgment calls).
  - Tests (1): `painted_at_size` re-pinned (below).
- Tests (names → what each pins)
  - `fit_channel_identity_passes_bytes_through_byte_exact` — a same-size
    fit returns the input bytes unchanged.
  - `fit_channel_keeps_a_constant_image_constant_on_upsample` — a 2×2
    constant (77,77,77,255) stays exactly that through a 4×4 fit.
  - `fit_channel_upsamples_a_ramp_to_the_hand_derived_bytes` — a 2×1 gray
    ramp [0, 255] fit to size 4 yields the exact edge-clamped bilinear
    bytes `0, 64, 191, 255` in every 4×4 row.
  - `fit_channel_renormalizes_the_normal_channel` — flat (128,128,255)
    stays flat after the normal renormalize, AND a non-unit
    (255,255,128) renormalizes to (218,218,128) while a `baseColor` fit of
    the same bytes stays (255,255,128), proving `channel` is honored.
  - `painted_at_size_is_now_just_session_presence` (RE-PINNED into
    existence — the fn had no direct test) — `Some((512,512))` and
    `Some((4096,4096))` are both `true` (any size), `None` is `false`.
  - `painted_tile_source_fits_a_target_of_another_size` (RE-PINNED,
    renamed from `painted_tile_source_skips_a_target_of_another_size`) —
    a 512² target requested at 512 is byte-exact length, and at 256 is FIT
    (`Some`, 256²·4), not `None`. GPU-gated.

Hand derivation: `fit_channel_to_export`'s arithmetic was checked against
a Python port of `resample_image_rgba8` + `f32_to_u8` before pinning — the
port reproduced `[0,64,191,255]`, constant 77, flat-normal 128/255, and
(218,218,128)/(255,255,128).

## CLI verdict (contract item 5)

GREP of `crates/umber-cli/src/main.rs`: the export path builds each tile's
`umber_export::MapSet` from bakes only — AO (`bake_ao`, main.rs:964-966)
and the baked tangent normal (`bake_tangent_normal_mesh`, 969-977). There
is NO paint session and no painted readback anywhere on the CLI export
path (the only `paint` match in the file is an unrelated doc comment at
:324). Verdict: **the CLI has no painted-channel drop**; lane A1 is a
dialog-path-only bug. No CLI change made.

## Behavior change (named, contract item 2)

`painted_at_size` and both painted readbacks changed contract:

- Before: a session whose target size ≠ the export size read as
  `FlatPlaceholder` in the badge and declined to `None` in the readbacks,
  so Base Color and every painted channel fell back to flat/baked bytes.
- After: a live session always reads as `Painted`; a present target is
  read back and FIT (bilinear) to `size×size`. Only a genuinely absent
  target/session still declines.

The one GPU test that pinned the old decline was deliberately re-pinned
(see the renamed test above); this is the named behavior change, not a
silent test edit.

## Judgment calls (alternative rejected)

1. **`fit_channel_to_export` takes `Vec<u8>` (owned), not `&[u8]`.** The
   identity arm then returns the buffer as-is — a true no-copy
   passthrough, matching the brief's "identity passes bytes through
   EXACTLY, no copy semantics change beyond the fit". Rejected `&[u8]` +
   `to_vec()`: an extra copy on the identity path for no benefit.
2. **Kept the name `painted_at_size`** while dropping its `size` argument
   and decline. Rejected renaming to e.g. `paint_session_live`: the brief
   refers to the fn by this name, and the dragon reviews against those
   anchors; the doc records that the name is now historical.
3. **Kept the badge's `FlatPlaceholder => match paint_size { Some(..) => … }`
   arm** even though a live session now always reads as `Painted`, making
   the `Some` arm unreachable. Rejected deleting it: rule 6's dead-code
   discipline (wire, don't delete) and the i18n constraint — the arm's key
   `export.source-flat-size` must stay in both `en-US.ftl` and `pt-BR.ftl`
   (`every_shipped_locale_covers_every_en_us_key` forbids divergence), and
   neither `.ftl` is in this kill's file surface. The arm is harmless and
   defensive if the pick ever re-gates on size.
4. **The fit targets `size×size`.** The brief's "2×1 ramp upsampled to
   4×1" therefore lands as a 4×4 fit; because a 1-tall source clamps to
   row 0 for every destination row, each 4×4 row is exactly the derived
   4×1 ramp (asserted as `row.repeat(4)`). Rejected an aspect-preserving
   fit: the export needs `size²×4` (`apply_base_color_with_graph` checks
   that length; the driver's `validate_set` enforces one size per MapSet),
   and per-channel native-size export is the named deferral.
5. **Read back, then fit.** Previously a mismatched size was declined
   BEFORE the readback. Now any present target is read back and then fit.
   Accepted cost: an 8K target exported at 1K now pays an 8K readback
   before the fit. Rejected keeping the pre-readback gate: the fit needs
   the bytes.
6. **Re-pinned the GPU decline test at 512→256**, not 512→8192. Rejected
   reusing 8192: the fit would allocate a 256 MiB output in the test for
   no added coverage — the exact resample bytes are already pinned
   headless.

## Unverified (no cargo per discipline)

- **GPU-gated tests not run.** `painted_tile_source_fits_a_target_of_another_size`
  (re-pinned) plus the untouched GPU tests `painted_roughness_reaches_the_exported_orm_png`
  and `each_tiles_paint_exports_to_its_own_file` need a wgpu adapter. The
  latter two call the readbacks with target size == export size, so the
  fit is the byte-exact identity arm and their behavior is unchanged — an
  argument, not a run. The re-pinned test additionally reads a fresh
  512² target back and fits to 256².
- **f32 vs the Python port.** The port is double precision; the pins sit
  at values with wide rounding margins (63.75→64, 191.25→191, 254.998→255,
  217.66→218), so f32/f64 cannot flip the `round`. Stated, not proven by
  an actual Rust run.
- **fmt/clippy not run** (binding discipline). The new `fit_channel_to_export`
  signature is pre-wrapped the way rustfmt breaks a >100-col signature; the
  dragon gates rustfmt.

## Deferred

- **Box-filter minification.** Bilinear downsampling (e.g. 4K→1K)
  undersamples and aliases; a proper box filter is the named deferral.
  Recorded in the module docs of both touched files.
- **Per-channel native-size export of passthrough files.** Export always
  writes at the export size; packed outputs force one size anyway
  (`validate_set`, `crates/umber-export/src/driver.rs:584`).
- **`apply_baked_normal` still declines a differently-sized baked normal**
  (`bake_sources.rs`)— it does not fit. Out of scope: the brief named only
  the two PAINTED readbacks. Left as-is deliberately.

## Process note

- Per the brief, only the three owned files were written; no cargo, no
  git mutations, no lock operations, no `/tmp` writes. The requirements
  row (`docs/specs/requirements.md`) is not in this kill's file surface,
  so it is not marked here — the dragon fills it.

## One-screen summary

Files changed:
`crates/umber-app/src/bake_sources.rs`,
`crates/umber-app/src/export_dialog.rs`, this report.

Tests (6; 5 new + 1 re-pinned):
- `fit_channel_identity_passes_bytes_through_byte_exact` — identity fit is
  byte-exact passthrough.
- `fit_channel_keeps_a_constant_image_constant_on_upsample` — constant
  image stays constant on 2→4.
- `fit_channel_upsamples_a_ramp_to_the_hand_derived_bytes` — 2×1 ramp
  [0,255] → exact edge-clamped bilinear `0,64,191,255` per row.
- `fit_channel_renormalizes_the_normal_channel` — flat (128,128,255)
  stays flat; non-unit (255,255,128) → (218,218,128) under `normal` but
  unchanged under `baseColor`.
- `painted_at_size_is_now_just_session_presence` (RE-PINNED) — any target
  size is `true`, `None` is `false`.
- `painted_tile_source_fits_a_target_of_another_size` (RE-PINNED) — a
  present target of another size is fit (`Some`), not declined (`None`).

Contract closed: both painted readbacks fit a differently-sized target to
the export size through `resample_image_rgba8`; `painted_at_size` is now
"a session exists"; identity is byte-exact; the CLI has no painted
channel to drop; aliasing/box-filter deferral documented.
