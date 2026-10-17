# Kill report: psd-publish (C2-32's named follow-up, data half)

- Authorship: dragon (solo, host-down pass).
- Kill-lock `psd-publish`: `crates/umber-app/src/psd_import.rs`.
- Closes the follow-up named in
  `2026-10-16-import-srgb-linearize`: "PSD's publish rule, when its
  pixels reach a plane, applies the same rule — linearize at the plane
  boundary." The publish PRIMITIVE lands now; the shell binding
  (`publish_plane` at tile 1001 with the set's name) is queued on the
  main.rs lock with the other shell halves.

## Built

- `publish_psd_pixels(flat: &PsdImport) -> Vec<u8>`: the C2-32 rule
  for PSD — the reader's pixels are Photoshop's gamma-space composite
  (the module's blend rule keeps bytes as given), so RGB linearizes
  through the SHARED curve (`curvature_png_to_linear`, the exact fn
  the mesh-map publish uses) BEFORE premultiply; alpha untouched;
  then `straight_to_premultiplied`. A PSD flat is always a colour
  image — no per-channel kind check, unlike the mesh-map importer's
  `channel_kind_for`.
- Tests (2): the pinned decision sRGB 188 → linear 128 → premultiplied
  by alpha 128/255 = 64 (expectations confirmed against a Python port
  of the EXACT shared curve + f32_to_u8 rounding before pinning: the
  first draft's `128 * 128 / 255` expression happened to match but was
  replaced with the literal `64` for clarity); the curve's endpoint
  invariants (0→0, 255→255, alpha 255 = identity premultiply).

## Judgment calls

- **Reuse, twice over**: `curvature_png_to_linear` is the same shared
  curve the C2-32 fix used (one curve, named after its first
  consumer — the skill's own reuse rule), and
  `straight_to_premultiplied` is the material-layer publish's own
  premultiply. The PSD rule is a composition of existing, tested
  primitives, not new math.
- **Pure fn, not a PaintState method**: the publish DECISION is bytes
  in/out; the plane binding (tile 1001, PlaneOwner, size refusal) is
  the shell's existing `publish_import_plane` pattern — the lock-held
  half. Keeping this pure means the C2-32 semantics are testable
  without a GPU session.
- **Path audit note**: `material_layer` lives in umber-PAINT (not an
  app module) — the first draft's `crate::material_layer` path was
  wrong; caught by reading the module's location, reverted to
  `umber_paint::material_layer`.

## Unverified

- Battery (host down since 18:25). Static: rustfmt PASS; call paths
  verified (`umber_paint::paint_state::curvature_png_to_linear` —
  the same path main.rs:1296 uses; `umber_paint::material_layer::
  straight_to_premultiplied` — module located at
  crates/umber-paint/src/material_layer.rs:1257); expectations
  Python-ported against the exact curve. Ad-hoc verified, not
  suite-verified.

## Deferred

- The shell binding: `publish_plane(PlaneOwner::Layer(id), 1001,
  baseColor, w, h, publish_psd_pixels(&flat))` in the PSD drop's
  apply path (main.rs, lock-queued).
