# Design brief: umber-vector-miter-limit (K8b, 2026-11-21)

Kill lock: `razul-vector-miter-limit`.
Base: K8a report/origin `075bc104`.

## What exists

`Stroke` supports Miter/Round/Bevel joins. `join_polygon` computes a miter tip
and falls back to bevel beyond a hardcoded `4 * radius` distance. SVG import
accepts only the default `stroke-miterlimit=4`; non-default values are typed
refusals at root, group, and path. Export has no miter-limit attribute.

Normative source: W3C SVG 1.1 (Second Edition), Painting §11.4,
https://www.w3.org/TR/SVG11/painting.html#StrokeMiterlimitProperty .
The limit is the ratio `miterLength / stroke-width = 1 / sin(theta/2)`,
must be >= 1, and defaults to 4. Miter length is outer tip to inner corner.
The first threshold fixture confused direction change with SVG's interior
angle. Independent review corrected the model: for direction change `delta`,
SVG's angle is `pi - delta`; a 20-degree direction change has ratio about
1.015, not 5.759. The old heuristic agrees with the default outcome in both
review probes, so no default-geometry defect is claimed. K8b computes the
normative ratio directly as `2 / |u + v|`, adds scaled inclusive-boundary
tolerance, and normalizes intersection directions for scale independence.

## Target

K8b adds bounded configurable miter limits:
- Add additive `Stroke.miter_limit: f32`, default 4, omitted from JSON when 4
  so old projects remain byte-compatible. Include it in NaN-stable cache
  identity. Reject non-finite or <1 values with typed errors.
- Pass the configured limit through solid and dashed stroke geometry. Preserve
  the existing default threshold and add direct threshold tests around a sharp
  angle, including exact-limit acceptance and below-limit bevel fallback.
- Parse finite `stroke-miterlimit >= 1` from root/group/path presentation
  attributes with normal inheritance, override, empty/inherit preservation,
  and sibling isolation. Replace non-default refusal with typed invalid-value
  refusal only.
- Export `stroke-miterlimit` when non-default, including non-Miter strokes for
  lossless state roundtrip. Preserve byte-identical default export.
- Keep compound contours, grouped undo/redo, strict single-shape behavior,
  cache invalidation, and all existing cap/join/dash semantics intact.

## Concurrency and safety

Locked files: core `vector.rs`, `svg.rs`, `vector_svg.rs`, `project.rs`,
`layers.rs`; app `vector_panel.rs`; this brief/report, queue/STATE/ROADMAP,
coordination, and generated README scorecard. No schema/project-version bump,
dependency, UI redesign, release action, public PR, or work outside the kill.

## Verification

Tests cover direct miter-vs-bevel thresholds, default preservation, exact-5
and exact-50 boundary acceptance and next-down rejection, near-reversal finite
limits, rotations, both windings, closed joins, small/large-scale intersections,
public solid/dashed rasters, invalid
and NaN limits, masked-invalid ancestors, cache identity, deterministic
old/new JSON, root/group/path inheritance including empty/inherit preservation,
Round/Bevel/Miter export/import/export byte stability, strict single-shape
behavior, compound contours, full core/app battery, and bounded independent
review.

## Non-goals

No miter-clip, arcs joins, CSS style parsing, UI control, release work, or
public PR.
