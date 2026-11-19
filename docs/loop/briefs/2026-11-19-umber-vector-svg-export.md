# Design brief: umber-vector-svg-export (K5a, 2026-11-19)

Kill lock: `razul-vector-svg-export`. K1 `46d4bff5`, K2 `0b5cb72d`, K3
`98fdbb80`, and K4 `a4373c51` are landed. The remaining SVG work splits into
K5a deterministic export and K5b import conversion, so each has a bounded
verification contract.

## What exists

- A vector layer is one `VectorShape`: ordered cubic anchors, closed flag,
  sRGBA fill, and optional sRGBA round stroke in UV units.
- K2 maps UV directly to device x/y and uses nonzero fill, round caps, and
  round joins. K4's Vector Properties section already displays every layer.
- The existing `svg.rs` parser is raster-first and keeps no Bézier AST. K5a
  needs no parser change. K5b will address geometry-sink reuse separately.

## Target and decisions

1. **Deterministic core serializer.** New `umber_core::vector_svg` exposes
   `export_svg(&VectorShape) -> Result<String, VectorSvgError>`. It writes one
   SVG 1.1 document with `width="1" height="1" viewBox="0 0 1 1"` and one
   `path`: `M`, `L` for two collapsed handles, `C` otherwise, and `Z` only
   when closed. Numbers use nine fractional digits, trimmed trailing zeros and
   decimal point, with negative zero normalized to zero. Non-finite geometry
   and anchor counts above `MAX_VECTOR_ANCHORS` are typed refusals.
2. **K2-equivalent paint attributes.** Open paths and zero-alpha fills emit
   `fill="none"`; painted closed paths emit hex `fill` plus `fill-opacity`.
   Painted strokes emit hex `stroke`, `stroke-opacity`, UV `stroke-width`,
   `stroke-linecap="round"`, `stroke-linejoin="round"`, and nonzero fill rule.
   Absent, zero-width, non-finite-width, or zero-alpha strokes emit
   `stroke="none"`. No gradient, dash, cap/join selector, or schema change.
3. **User-selected export.** Each vector layer's K4 section gains `Export SVG`.
   The button serializes the current params after applying any same-frame edit,
   opens the existing rfd save dialog with a sanitized layer-name filename,
   writes only the chosen path, and routes Saved/Failed outcomes through the
   shell's existing notice rail. Cancel is inert. No project file or telemetry.
4. **Bounded K5a.** K5b import, XML transforms, multiple subpaths, shape
   elements, and SVG color parsing are out. They are not silently claimed by
   this export slice.

## Tests

- Exact deterministic document golden for mixed line/cubic segments, closed
  path, alpha, round stroke attributes, and trimmed numbers.
- Open path forces fill none; no/zero-alpha/zero-width/non-finite stroke is
  stroke none; non-finite geometry and over-cap anchors are typed errors;
  repeated exports are byte-identical; negative zero canonicalizes.
- Filename suggestion is filesystem-safe, nonempty, bounded, and stable.
- Existing K4 panel and core/gui/app batteries remain green. No real file is
  written by tests; save-dialog cancellation and filesystem errors remain
  manual/platform paths, with the outcome mapping unit-tested where headless.

Battery: rustfmt, layers, scorecard, `git diff --check`, five-crate clippy and
tests under the shared flock. Bounded Kimi K3.1 Pro read-only review before
commit; kill report, queue, STATE, ROADMAP, coordination, task-progress, and
vault checkpoint at landing.

## Exact lock surface

- `crates/umber-core/src/lib.rs`, `crates/umber-core/src/vector_svg.rs` (new)
- `crates/umber-app/src/vector_panel.rs`, `crates/umber-app/src/main.rs`
- `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`
- `README.md` (scorecard only if its bucket changes)
- `docs/ROADMAP.md`, this brief, the K5a kill report, `docs/loop/queue.md`,
  `STATE.md`, `.dragon/COORDINATION-next-wave.md`

## Non-goals

No K5b import, multiple paths/subpaths, XML parser rewrite, arbitrary SVG paint
servers, public PR, release tag, version bump, or implicit filesystem writes.
