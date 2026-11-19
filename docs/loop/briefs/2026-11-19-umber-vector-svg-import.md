# Design brief: umber-vector-svg-import (K5b, 2026-11-19)

Kill lock: `razul-vector-svg-import`. K5a deterministic export is `09d01fbd`
with closeout `f09fdd14`. This is the bounded import half of the path-tool SVG
work, not a claim of full SVG support.

## What exists

- K5a writes a root `<svg>` with one `<path>`, UV coordinates, M/L/C/Z geometry,
  hex sRGBA fill/stroke, opacity, and round stroke attributes.
- `svg.rs` has a strict XML tokenizer, root/viewBox geometry, relative and
  smooth path-command handling, and a `build_path` walker that currently emits
  only flattened `PolyBuilder` geometry.
- Vector layers hold one continuous path, so multiple shapes/subpaths and arcs
  need explicit refusal rather than silent loss.

## Target and decisions

1. **Geometry-sink split, old raster unchanged.** Generalize `build_path` over
   a crate-private sink. `PolyBuilder` keeps its existing move/line/cubic/quad/
   arc behavior; a new import sink retains anchors. Existing SVG raster output
   and tests must stay byte-identical.
2. **Bounded document subset.** Accept one root `<svg>` containing one `<path>`
   (plus skipped metadata/unknown subtrees under the existing policy). Support
   M/L/H/V/C/S/Q/T/Z and relative forms through the existing walker. Refuse
   multiple paths, multiple subpaths, shape elements, groups, arcs, evenodd,
   and unsupported paint servers with typed `SvgError`s. No silent conversion.
3. **UV mapping and paint.** Reuse `root_geometry` for viewBox/root transforms,
   then normalize device x/y by root width/height. Fill/stroke accept `none`,
   #RGB, and #RRGGBB plus opacity; missing fill is opaque black and missing
   stroke is none. Nonzero only. Stroke width uses the root CTM's `max_scale`
   divided by `min(width,height)`, matching K2's min-dimension convention and
   documenting that non-uniform stroke transforms are approximate.
4. **One grouped undoable import.** Vector Properties gains Import SVG. The app
   checks metadata against `MAX_SVG_INPUT_BYTES` before reading, parses the
   selected file, then uses one `Document::run_grouped(AddVector)` containing
   Add + SetVector. The shell selects the new layer and its Vector context.
   Cancel and read/parse failures create no layer; outcomes use the notice rail.
5. **Round trip with K5a.** Exported canonical documents import back to the
   same anchors, closed flag, fill, and stroke (within f32 formatting). This is
   a real vector round trip, not raster coverage equivalence.

## Tests

- Existing `svg.rs` raster suite stays green, including exact coverage goldens
  and hostile-input budgets.
- K5a export → K5b import → export is byte-identical for line/cubic/open/closed
  and alpha/stroke cases.
- Relative/smooth quadratic conversion produces the exact cubic controls;
  root viewBox maps to UV; #RGB expands; opacity quantizes like K5a.
- Multiple paths/subpaths, groups, arcs, evenodd, unsupported paint, non-finite
  mapped geometry, and over-cap anchors are typed refusals.
- App input helper checks metadata before reading and leaves the document
  unchanged on failure. The grouped AddVector history is one undo step.

Battery: rustfmt, layers, scorecard, `git diff --check`, five-crate clippy and
tests under the shared flock. Bounded Kimi K3.1 Pro read-only review before
commit; kill report, queue, STATE, ROADMAP, coordination, task-progress, and
vault checkpoint at landing.

## Exact lock surface

- `crates/umber-core/src/svg.rs`, `crates/umber-core/src/vector_svg.rs`
- `crates/umber-app/src/vector_panel.rs`, `crates/umber-app/src/main.rs`
- `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`
- `README.md` (scorecard only if its bucket changes)
- `docs/ROADMAP.md`, this brief, the K5b kill report, `docs/loop/queue.md`,
  `STATE.md`, `.dragon/COORDINATION-next-wave.md`

## Non-goals

No full SVG, CSS, gradients, multiple shapes/subpaths, arc-to-cubic fitting,
text/images/use/defs, raster stencil behavior changes, implicit file reads,
release tag, version bump, or public PR.
