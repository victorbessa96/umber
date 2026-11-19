# Kill report: umber-vector-svg-import (K5b, 2026-11-19)

Authorship: dragon implementation and verification, with bounded independent
read-only review by Kimi K3.1 Pro. Kill lock: `razul-vector-svg-import`. The
lock held `crates/umber-core/src/{svg,vector_svg}.rs`,
`crates/umber-app/src/{vector_panel,main}.rs`, both i18n catalogs, `README.md`,
`docs/ROADMAP.md`, the K5b brief, this report, `docs/loop/queue.md`, `STATE.md`,
and `.dragon/COORDINATION-next-wave.md`.

Implements `docs/loop/briefs/2026-11-19-umber-vector-svg-import.md`: Target and
decisions 1–5, tests, and exact lock surface. K5a export is `09d01fbd`; K5b
code landing is `0d1b6ad7`.

## Built

- A crate-private `PathSink` split in `svg.rs`. The existing `PolyBuilder`
  keeps its flattening behavior; the new `VectorImportBuilder` retains exact
  cubic anchors instead of raster polygons. The original SVG raster suite
  stays green.
- `umber_core::vector_svg::import_svg`: bounded one-root/one-path import with
  M/L/H/V/C/S/Q/T/Z and relative forms through the existing path walker.
  Quadratic and smooth-quadratic segments elevate exactly to cubics. Multiple
  paths/subpaths, groups, shape elements, arcs, evenodd, path transform/style,
  unsupported paints, non-finite mapped UVs, and over-cap anchors are typed
  `SvgError` refusals.
- Root composition matches the raster walker: `root_geometry` for width and
  viewBox, then `apply_presentation` once for root transform. The import sink
  maps that single CTM to UV; path transforms are refused before the walker.
- Paint subset: `none`, #RGB, #RRGGBB, fill/stroke opacity, nonzero fill, and
  round-cap/join painted strokes. Stroke width uses the root CTM max scale over
  the root's minimum dimension, matching K2's convention.
- Explicit SVG closing points fold into the first anchor's incoming handle, so
  K5a's closed-path export does not import a duplicate first anchor. Canonical
  K5a export → K5b import → export is byte-identical.
- Vector Properties gains Import SVG. The app checks metadata against
  `MAX_SVG_INPUT_BYTES` before reading, parses before touching the document,
  and installs Add + SetVector inside one `run_grouped(AddVector)`. The shell
  selects the new layer and its paint target. Cancel/read/parse failures create
  no layer; outcomes use localized notices in both catalogs.

## Judgment calls

- Kept the subset deliberately narrow: one continuous path, no CSS, no groups,
  no arc fitting, no paint servers, no multiple subpaths. Unsupported geometry
  fails loudly rather than silently changing the artist's shape.
- Root `transform` is supported through the same presentation composition as
  the raster path; per-path `transform` and `style` are refused. This avoids
  both transform omission and double application.
- No test opens the native open dialog. Parser, cap, grouped-history, and
  outcome logic are headless; native dialog behavior remains a manual check.

## Verification

- Vector SVG suite: **10 passed / 0 failed**, including exact K5a roundtrip,
  quadratic/T elevation, viewBox/#RGB/opacity mapping, root-transform-once,
  path-transform refusal, unsupported documents, and anchor-cap refusal.
- Original SVG raster suite remains green after the sink split.
- App vector panel suite: **6 passed / 0 failed**, including metadata budget,
  grouped Add+SetVector, one history row, and undo removing the imported layer.
- i18n suites green in both lib and bin targets; catalog parity and called-key
  checks pass.
- Final static gate: rustfmt clean; `check-layers.py` GREEN (16 crates);
  `scorecard.py --check` current; `git diff --check` clean.
- Final five-crate clippy + test battery: **clippy 0 errors; 19 suites, 1,943
  passed / 0 failed**. Receipt:
  `~/.hermes/cache/scratch/vector-svg-import-final-battery-r2.log`.
- Bounded Kimi K3.1 Pro round 1: **FIX-FIRST**, alleging double root-transform
  application. Live-code inspection disproved the double mechanism, but the
  requested regression exposed the real adjacent defect: import ignored root
  transform. Fixed by matching the raster walker's root presentation
  composition and pinned by exact-anchor tests. Kimi delta re-review:
  **SHIP**, no HIGH/MEDIUM finding. Receipt:
  `~/.hermes/cache/scratch/vector-svg-import-review-kimi-r2-inline.log`.

## Unverified

- No automated claim is made about the native open dialog, filesystem-error
  presentation, or third-party SVG interoperability beyond the exact canonical
  byte roundtrip and subset tests.
- Full SVG/CSS, groups, multiple shapes/subpaths, arc-to-cubic fitting, paint
  servers, text/images/use/defs, and per-path transforms remain unimplemented
  and are not claimed.

## Deferred

- Richer SVG import coverage, multiple vector layers per document, cap/join/dash
  selectors, and release/version work.
- Fill cache-lifecycle parity remains the separate named follow-up.

## Process note

Kimi K3.1 Pro was the sole review lane. No claw edits, claw cargo, or claw
commits were used. One foreground re-review timed out without a verdict; a
stalled background duplicate was killed, and the final SHIP came from a tight
inline delta review of the exact fix and passing regression. Pi's default model
is `moonshotai/kimi-k3.1-pro`; the nemotron entry was removed from its NIM model
list per Bessa's order.
