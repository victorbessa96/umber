# Shipped project templates

Ten curated `.umber` starter projects that ship with Umber. Each one is a
real directory project (`<slug>.umber/project.json` +
`layers/<Set>/<id>.json`) that `umber_core::load_from_dir` accepts, so the
templates double as format examples.

A template carries **metadata, not pixels**: the `.umber` JSON stores the
texture sets, channels, layer stack, blend modes, opacities, masks and
visibility. Layers open blank — the template is the scaffold, not painted
art. (Live layer-owned pixel planes are project version 2; templates stay
at version 1 because they hold no planes. See
`LAYER_PLANES_PROJECT_VERSION` in `crates/umber-core/src/project.rs`.)

The loader/menu wiring is a separate change; today these are validated by
the `every_shipped_template_loads` test in `crates/umber-core/src/project.rs`.

## Index

| Slug | Name | Description | Sets / channels |
| --- | --- | --- | --- |
| `pbr-surface-kit` | PBR Surface Kit | Metal, wood and leather starting stacks on the full OpenPBR channel bundle — the everyday hard-surface base. | Metal, Wood, Leather — baseColor, roughness, metallic, normal, height, ambient_occlusion |
| `sci-fi-panel` | Sci-Fi Panel | Hard-surface panel and trim stacks with bevel highlights, greebles and edge dirt for kit-bashed props. | Panel — baseColor, roughness, metallic, normal, height, ambient_occlusion, emissive; Trim — same minus emissive |
| `terrain-height` | Terrain Height | Height-first terrain stack: ridges, erosion, grass mask, rock outcrops and snow. | Terrain — height, normal, baseColor, roughness, ambient_occlusion |
| `character-skin` | Character Skin | Face/body skin plus an eye set: pore detail, flush zones, blemish fix, sclera/iris/catchlight. | Skin — baseColor, roughness, normal, height, ambient_occlusion; Eyes — baseColor, roughness, normal |
| `stylized-toon` | Stylized Toon | Flat-color toon stack with shadow/highlight bands, an ink outline pass and a face-decal folder. | Toon — baseColor, opacity, roughness, normal |
| `ui-icons` | UI Icon Set | 512 px icon sheet: base pass, symbol shapes, glow and a hidden hover variant. | Icons — baseColor, opacity, emissive |
| `grunge-overlay` | Grunge Overlay | Reusable grunge sheet — dirt speckle, scratch pass and edge grime to overlay on any surface. | Grunge — opacity, roughness, baseColor, normal |
| `water-normals` | Water Normals | Tiling water surface: base ripples, cross swell, chop detail and a foam mask. | Water — normal, height, roughness, opacity |
| `emissive-sign` | Emissive Sign | Lit sign stack: housing, sign face, additive emissive tubes and a bloom mask. | Sign — baseColor, emissive, roughness, opacity |
| `weathering-kit` | Weathering Kit | Wear-and-tear passes — rust patches, streaks, chipped paint and dust film — over a clean PBR base. | Weathered — baseColor, roughness, metallic, ambient_occlusion, normal |

## Authoring rules (what makes a template shippable)

1. **It loads clean.** `cargo test -p umber-core every_shipped_template_loads`
   must pass with the count assertion unchanged. Never add a template that
   the test has to special-case.
2. **Version-pinned.** `"version": 1` only. Do not bump a template to 2
   unless it genuinely carries live layer-owned pixel planes (it cannot —
   templates ship no pixels); the app stamps version 2 at save time when a
   planed layer is live. The loader refuses any version outside
   `1..=MAX_SUPPORTED_PROJECT_VERSION`.
3. **A real directory project.** `<slug>.umber/project.json` plus
   `layers/<Set>/<N>.json`. The slug is kebab-case ASCII; the set name is
   the on-disk directory name and must be a safe, unique, non-reserved
   path segment (the loader's `validate_set_name` rejects `.`, `..`,
   Windows device names, reserved characters and trailing dots/spaces).
4. **Ids are honest.** Layer ids start at 0 and are sequential per set, the
   file name equals the layer's `id` field, ids are unique within the set,
   and `next_layer_id` is `max(id) + 1`. The loader rejects a missing file,
   a duplicate id, or an id/file mismatch.
5. **Canonical channels.** Use the names the export/bake lanes know —
   `baseColor`, `emissive` as `Color`; `roughness`, `metallic`,
   `ambient_occlusion`, `normal`, `height`, `opacity` as `Data`. Extra
   channels are legal but must still be honestly typed.
6. **No absolute paths, no machine or user data.** `.umber` has no path
   fields; do not introduce one, and never embed a home directory, a
   username, a hostname or a secret.
7. **No third-party pixels or text.** Templates ship metadata only. Do not
   paste reference art, brushes, font bytes or copied prose into layer or
   project JSON.
8. **Keep them small.** One to three sets, a handful of layers (≤ 8) per
   set. These are starting points, not finished assets. The loader's 8 MiB
   `project.json` / 64 MiB layer-file caps are hostile-input limits, not a
   size budget.
9. **Valid enums only.** `BlendMode` serializes as its PascalCase variant
   name (`Normal`, `Multiply`, `Screen`, `Overlay`, `Add`, `SoftLight`, …;
   `Passthrough` on a folder). `LayerKind` is `Paint`, `Fill`, `Material`,
   `{"Instance": {"source_id": n}}` or `{"Folder": {"passthrough": bool}}`.
   A `Fill`/`Material` layer must carry its payload (`fill`/`material`), so
   templates use `Paint` and `Folder` scaffolding only.
10. **Update the index and the count together.** A new template means a new
    row here and a bumped count assertion in the test — the two are the
    release contract for the shipped set.
