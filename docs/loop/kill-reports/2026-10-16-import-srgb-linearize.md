# Kill report: import colour PNGs linearize into the paint planes (C2-32)

Date: 2026-10-16. Author: dragon. Lock: `import-srgb-linearize`
(`crates/umber-app/src/main.rs`, `crates/umber-app/src/mesh_map_import.rs`,
this report). C2 audit row 32.

## Built

- `publish_import_plane` (main.rs): when the applied channel's kind is
  `ChannelKind::Color` (via the existing `mesh_map_import::channel_kind_for`),
  the decoded PNG's RGB linearizes with
  `umber_paint::paint_state::curvature_png_to_linear` — the shared sRGB→linear
  curve, alpha/coverage untouched — BEFORE `straight_to_premultiplied`.
  Data channels (AO, roughness, height, ...) pass through unchanged: they are
  unit-range values, not encoded colour.
- The bug: planes are LINEAR (picker and eyedropper linearize on the way in;
  export re-curves baseColor/emissive). Publishing sRGB bytes as-is curved
  them twice on export (washed out) and rendered them too light in the
  viewport.
- Test (mesh_map_import): the decision pin — `channel_kind_for("baseColor")`
  is Color and linearizes sRGB 188 grey to linear 128 (the export re-curve's
  exact inverse anchor); `channel_kind_for("roughness")` is Data and bytes
  pass through. The transform's full 0..=255 round-trip is already pinned in
  umber-paint (`curvature_png_decode_inverts_the_bake_writers_srgb_transfer`).

## Judgment calls

- Reused `curvature_png_to_linear` rather than writing a new helper: it IS
  the sRGB→linear curve with alpha untouched — the name is
  curvature-specific, the transform is not. A later rename to a shared
  `srgb_to_linear_rgb` would be cosmetic; noted, not done (no behavior
  change wanted in this kill).
- The premultiply happens AFTER linearization: straight-alpha colour must
  linearize before unpremultiply math; the order is the correct one and
  matches the bake-read site (main.rs:514's anchor path).
- The sibling bake-read site (main.rs ~505) linearizes by a NAME allow-list
  (`normal`/`height` skip, everything else linearizes). That heuristic stays
  as-is: changing it is behavior drift outside this kill's surface; the two
  sites agree that color-ish data linearizes.

## Unverified

- `publish_import_plane` itself needs a GPU PaintState; the end-to-end
  (import → plane → export 188) rides the wave battery's suite coverage of
  the import path plus the pinned transform and decision. No new E2E
  (loop-prompt rule 2: Gate 0 is the one E2E harness).

## Deferred

- The audit row's second half — the spp extract writer switching its colour
  output to sRGB (it writes linear today to round-trip through this gap) —
  is a separate kill on the spp-extract surface (`spp-extract-cli`'s writer),
  now UNBLOCKED by this fix landing. Named follow-up.
- PSD import's publish, when its pixels reach a plane, applies the same rule
  (its module docs keep bytes-as-given for gamma-space blending; the publish
  linearizes at the plane boundary, exactly like this kill). Named follow-up
  with the PSD-publish work itself.
