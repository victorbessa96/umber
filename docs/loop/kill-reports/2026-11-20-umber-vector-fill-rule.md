# Kill report: umber-vector-fill-rule (K6a, 2026-11-20)

Authorship: dragon implementation and verification, with bounded read-only review by `opencode/beam-free`. Kill lock: `razul-vector-fill-rule`.

Base: Fill cache-lifecycle `8747c681`, pushed truth `4abfb021`, native smoke record `24acff92`. Brief: `894a8e14`. Code landing: `7372aa53`.

Implements `docs/loop/briefs/2026-11-20-umber-vector-fill-rule.md`: additive even-odd vector fill with panel and SVG roundtrip support.

## Built

- `FillRule` is public with Serialize/Deserialize/Default. `VectorShape` carries `fill_rule` additively, so old projects load as nonzero and cache identity includes the rule.
- `rasterize_polys_rule` uses the existing scanline while `rasterize_polys` preserves nonzero behavior for all old callers.
- Vector Properties has a Nonzero / Even-odd combo through one undoable `SetVector` edit and both i18n catalogs.
- SVG export writes the configured rule. `parse_vector_paint` accepts nonzero/evenodd and inherits root_style.rule; other values are typed Unsupported.
- Star test proves inner pentagon differs only as expected, including full coverage arrays against the SVG oracle. `evenodd_exports_and_imports_as_the_same_rule` covers export/import and root inheritance.

## Verification

- Focused core vector suite: **20 passed / 0 failed**.
- Focused vector_svg suite: **17 passed / 0 failed**.
- Focused app vector_panel suite: **10 passed / 0 failed**.
- Full five-crate battery: **clippy 0 errors; 21 suites, 2,176 passed / 0 failed**. Receipt: `~/.hermes/cache/scratch/vector-fill-rule-final-battery-r2.log`.
- Review: **SHIP** from `opencode/beam-free`, no HIGH/MEDIUM defects. Its test-honesty note was closed by adding the full-oracle difference proof. Receipt: `~/.hermes/cache/scratch/vector-fill-rule-review-beam.log`.

## Deferred

- Cap/join selectors remain K6b; dashes, multi-shape layers, and project-version work remain out of scope for this kill.
