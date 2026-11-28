# UI/UX lens: late-review reconciliation (2026-11-28)

Review source: `deleg_320688ac-4`, targeting `ec979743`. Reconciled to current code. Six findings were already closed; six live UI/UX items were fixed with regressions, and the three ROADMAP claims were corrected. No actionable finding remains at current HEAD.

## Finding dispositions

| # | Disposition | Current evidence |
|---|---|---|
| 1 — Pressure curves are not keyboard-operable | **Stale: already fixed** | `CurveEdit` exposes named point controls, arrow-key movement, Delete/Backspace, and focus retention. Existing keyboard regressions cover real curve changes. |
| 2 — Graph output and curve handles fail Light contrast | **Stale: already fixed** | `output_accent` and non-hovered curve handles use `graphic_fg`, not hard-coded gold/white. Focus uses the theme focus ring. |
| 3 — Dock leaves can shrink below 120 pt | **LANDED** | `floored_fraction` enforces 120 where possible and stays valid below 240. `compact_for_width_with_rails` covers boot/custom saved layouts, width hysteresis, and Reset Layout. Tests cover 800/960/1100 plus tiny invalid splits and exact 120 values. |
| 4 — Glyph buttons expose only symbols | **Stale: already fixed** | Reorder arrows, material refresh, post-effect removal, and saved-search deletion use localized `glyph_button` accessible labels. |
| 5 — Fold-rail glyphs collide with toolbar actions | **LANDED** | Bakes uses `file`; Graph uses `panel-left`. `fold_rail_glyphs_do_not_collide_with_toolbar_actions` covers the full side-rail/top-strip census. |
| 6 — No-mesh panels have no Open action | **LANDED** | Shared `no_mesh_action` provides Open Mesh; both panels' kittests click it and drain once into `home_actions`. |
| 7 — Raw locale tags and untranslated literals | **LANDED** | Quick Setup uses the shared native-name mapper. Solid-dash hint and parser use the localized alias with case folding. The flattened single-word guard catches remaining literals. The old bake `Image files` literal is already absent. |

## Stale-document dispositions

- AssetStore GC and the icon atlas were already accurately documented in requirements and parity.
- ROADMAP now records the completed OccluderScope kittest.
- ROADMAP now records SVG K6c through K8i instead of stopping at K6b.
- ROADMAP no longer claims only U5 remains: U5 and desktop checks 1–4 are open, while F12/F13/F15/F17 and R3-1/R3-5 are closed here. F16 remains a deferral.

## Verification

- Full final app suite: **939 passed, 0 failed**; every integration target green.
- Strict app/gui/export/CLI clippy, rustfmt, scorecard (**3,360**), layers, parity, and `git diff --check`: clean.
- Independent review: round 1 FIX-FIRST findings fixed, round 2 Reset Layout finding fixed, round 3 **SHIP**.
- Targeted Windows CI is recorded separately; no unfiltered full-matrix claim is made here.
