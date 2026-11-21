# Design brief: umber-vector-visibility (K8d, 2026-11-21)

Planned kill lock: `razul-vector-visibility`.
Base: K8c implementation `803d6a35a8c1987896cf4d006649377e7290c2e7`; landed
status tip `5276be476de7addaf64ff012e77a742e0826e99f`.

## What exists

`import_vector_svg_many` in `crates/umber-core/src/svg.rs` walks root, nested
`<g>`, path, and basic-shape elements through scoped paint and effective-CTM
frames. K7c's inheritance and K7d's transforms are already bounded and ordered.
Root, group, and shape `display`/`visibility` attributes are currently typed
`SvgError::Unsupported` refusals (`refuse_root_import_presentation`,
`refuse_group_import_effects`, and `refuse_shape_import_effects`). The walker
returns the existing typed `no supported vector geometry in vector import`
error when no shapes survive. The panel consumes `import_svg_many` and installs
all returned layers in one undo group.

Normative source: W3C SVG 1.1 (Second Edition), Painting §11.5,
https://www.w3.org/TR/SVG11/painting.html#VisibilityControl . `display` is not
inherited; `display:none` removes the element and its children from direct
rendering, and any value other than `none` or `inherit` renders. `visibility`
has initial value `visible`, is inherited, and accepts `visible`, `hidden`,
`collapse`, and `inherit`; a child with `visibility:visible` can render beneath
a hidden group.

## Target

K8d implements display/visibility filtering for the **editable vector import**
subset, without changing the raster SVG importer or the vector model:

- Parse `display` and `visibility` at root, group, path, and basic-shape
  elements before geometry construction. Remove those two keys from the
  unsupported-effect lists; every other existing refusal remains in force.
- `display:none` prunes that element's direct-rendering subtree. A group with
  `display:none` pushes no active frame, so descendants cannot re-enable it.
  A hidden shape does not build geometry or consume shape/contour/anchor
  budgets. `display` absent, empty, or `inherit` leaves the current rendering
  state unchanged; every other value renders, per SVG 1.1 §11.5.
- Carry effective `visibility` in the existing scoped root/group frame.
  Absent, empty, or `inherit` preserves the parent value; `visible` renders;
  `hidden` and `collapse` suppress the current shape. A child `visible`
  overrides a hidden group. Visibility does not prune group traversal.
- Validate the remaining presentation attributes normally even on hidden
  elements. Once an element is known non-rendering, malformed path data is not
  parsed because no editable geometry is produced. XML tokenization and the
  document walk remain strict.
- Mixed visible/hidden documents return only visible shapes in original
  document order. An all-hidden document keeps the existing typed
  `no supported vector geometry in vector import` refusal rather than creating
  an empty project or silently succeeding.
- Hidden shapes do not count against shape/contour/anchor budgets; visible
  shapes retain every existing exact budget and failure-atomicity behavior.
- The existing panel path needs no production change: filtered shapes already
  flow through its one-history-group import. Add a focused panel regression
  proving only visible layers are installed and undo removes them together.

## Concurrency and safety

Locked files planned: `crates/umber-core/src/svg.rs`,
`crates/umber-core/src/vector_svg.rs`, `crates/umber-app/src/vector_panel.rs`,
this brief, the K8d kill report, `docs/loop/queue.md`,
`.dragon/COORDINATION-next-wave.md`, `STATE.md`, and `docs/ROADMAP.md`.
No vector schema/model field, native serialization, SVG export, raster SVG
walker, dependency, release, public-PR, or out-of-lock work.

## Verification

- RED first: focused visibility tests fail against the current typed refusals
  before implementation, then pass afterward.
- Core tests cover: display-none root/group/path pruning; non-none display
  values rendering; inherited visible/hidden/collapse/inherit; a visible child
  beneath a hidden group; sibling isolation and document order; all-hidden
  typed refusal; malformed hidden path data not parsed; other unsupported
  effects still refused; hidden geometry excluded from budgets.
- Panel test covers visible-only layer installation in one undo group.
- `rustfmt --edition 2021 --check` on touched Rust, `check-layers.py`,
  `scorecard.py --check`, strict clippy for `umber-core` and `umber-app`, and
  full `cargo test -p umber-core -p umber-app` under `/tmp/umber-cargo.lock`
  with the app-only `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber` override.
- Bounded independent review of the final diff, with findings fixed or
  explicitly disproved before landing.

## Non-goals

No group/element opacity, CSS `style`, clip/mask/filter/markers,
`vector-effect`, `paint-order`, events, references, text layout, clipping
geometry, or bounding-box model. Hidden SVG elements are filtered at import;
they do not become hidden Umber layers and are not preserved for re-export.
No rasterizer visibility behavior change, hidden-layer UI, schema/project
version change, release tag, or public posting.
