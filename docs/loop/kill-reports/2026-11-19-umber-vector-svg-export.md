# Kill report: umber-vector-svg-export (K5a, 2026-11-19)

Authorship: dragon implementation and verification, with bounded independent
read-only review by Kimi K3.1 Pro. Kill lock: `razul-vector-svg-export`. The
lock held `crates/umber-core/src/{lib,vector_svg}.rs`,
`crates/umber-app/src/{vector_panel,main}.rs`, both i18n catalogs, `README.md`,
`docs/ROADMAP.md`, the K5a brief, this report, `docs/loop/queue.md`, `STATE.md`,
and `.dragon/COORDINATION-next-wave.md`.

Implements `docs/loop/briefs/2026-11-19-umber-vector-svg-export.md`: Target and
decisions 1–4, tests, and exact lock surface. K1 is `46d4bff5`, K2 `0b5cb72d`,
K3 `98fdbb80`, K4 `a4373c51`, and the K5a brief is `18858292`. The K5a code
landing is recorded in the follow-up hash-truthing commit. K5b import remains
separate.

## Built

- New `umber_core::vector_svg::export_svg`: deterministic SVG 1.1 output with a
  1×1 UV viewBox and one path. Collapsed segment handles emit `L`; all other
  segments emit their exact cubic controls. Closed paths alone emit `Z`.
- Nine-fractional-digit numbers with trailing-zero trimming and negative-zero
  canonicalization. Non-finite geometry and over-cap anchors are typed
  `VectorSvgError` refusals.
- K2-equivalent paint attributes: nonzero closed fill, open/degenerate paths
  never paint, zero/one-anchor shapes emit no paint, and painted strokes carry
  UV width, sRGBA hex/opacity, and round caps/joins. Zero-alpha, zero-width,
  negative-width, and non-finite-width strokes emit `stroke="none"`.
- Each Vector Properties section gains `Export SVG`. Same-frame property edits
  commit before serialization. The existing rfd dialog suggests a bounded,
  filesystem-safe layer-name filename and writes only the user-selected path.
  Cancel is inert; Saved/Failed outcomes ride the shell's existing localized
  notice rail.

## Judgment calls

- Split K5 into export and import. K5a needs no parser change; K5b must deal
  with the existing raster-first parser's missing Bézier AST and is not silently
  claimed here.
- Kept SVG in UV units with a 1×1 viewBox, matching K2's direct device mapping
  and min-dimension stroke-width semantics without a second coordinate system.
- Used native rfd and the existing notices rather than adding a new export
  window, dependency, project field, or implicit filesystem write.
- No test opens the native dialog or writes a real file. Core goldens and the
  filename/outcome logic are headless; cancel and filesystem-error presentation
  remain named platform checks.

## Verification

- Focused serializer suite: **5 passed / 0 failed**, including exact document
  golden, byte-identical repeated export, line/cubic selection, alpha and
  number formatting, zero/one-anchor paint suppression, invalid strokes,
  non-finite geometry, and anchor-cap refusal.
- Vector panel suite: **4 passed / 0 failed**, including safe, bounded,
  nonempty filename suggestions and the export label in search labels.
- i18n suite after export strings: **15 passed / 0 failed**.
- Final static gate: rustfmt clean on every touched Rust file;
  `check-layers.py` GREEN (16 crates); `scorecard.py --check` current;
  `git diff --check` clean.
- Final five-crate clippy + test battery: **clippy 0 errors; 22 suites, 2,139
  passed / 0 failed; `FINAL_BATTERY_EXIT=0`**. Receipt:
  `~/.hermes/cache/scratch/vector-svg-export-final-battery-r1.log`.
- Bounded Kimi K3.1 Pro review of the exact export and save flow: **SHIP**, no
  HIGH/MEDIUM findings. Receipt:
  `~/.hermes/cache/scratch/vector-svg-export-review-kimi.log`.

## Unverified

- No automated claim is made about the native save dialog, a filesystem error
  dialog, or opening the exported file in third-party software. The exact SVG
  bytes are golden-tested; those platform interactions remain manual checks.
- K5b import, XML transforms, multiple subpaths, shape elements, and SVG color
  parsing are not implemented or claimed by this slice.

## Deferred

- K5b SVG-to-vector import through a geometry-sink split in the existing
  parser, with its own bounded subset and round-trip battery.
- Richer SVG paint servers, multiple vector layers per document, cap/join/dash
  selectors, and release/version work.
- Fill cache-lifecycle parity remains the separate named follow-up.

## Process note

Kimi K3.1 Pro was the sole review lane. No claw edits, claw cargo, or claw
commits were used. The first golden used the f64 spelling of 128/255; the test
was corrected to the actual f32 value `0.501960814`, not the serializer. A
self-review then added the zero/one-anchor paint-suppression guard and its
regression before the independent SHIP verdict and final battery.
