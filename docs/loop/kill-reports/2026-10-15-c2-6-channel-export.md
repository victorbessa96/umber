# Kill report: C2-6 export every painted channel

Date: 2026-10-15. Author: dragon. Lock: `c2-6-channel-export` over
`crates/umber-app/src/{bake_sources,export_dialog}.rs` and this report.
Source: `docs/loop/audit-2026-10-15-c2.md` C2-6, the "later slice" the
export dialog's module docs had named since wave 3 ("V1 painted scope is
Base Color ONLY"). Pairs with C2-5 (`c45d7cb`): painting any channel is
only useful if export writes it.

## Built (`bake_sources.rs`)

- `PAINTED_DATA_CHANNELS` maps each paint channel to its export map and
  its neutral: roughness → 128 gray, metallic → 0 (dielectric), height →
  128 (no displacement), normal → the flat (128, 128, 255), emissive →
  black. Emissive is the one entry outside `TextureSet::new_default`, a
  deliberate extra for a user channel by that name, and the docs say so.
- `flatten_over(premultiplied, neutral, size)` composites a channel's
  premultiplied paint over its opaque neutral: `paint + neutral·(1−α)`,
  rounded. Unpainted texels export the neutral exactly and painted ones
  the paint exactly. The output is always opaque, because a data map
  carries no transparency. A wrong-length buffer gives `None`, never a
  partial map.
- `apply_painted_channels` attaches each channel that materialized on
  the tile and returns which maps came from paint. A materialized but
  empty normal exports flat, not black. Channels nobody painted are not
  invented.
- `fill_unpainted_partners` closes the gap a first draft of this kill
  had. Painting roughness alone still dropped glTF's PACKED
  metallicRoughness output, because the packed output also needs
  metallic. For each output paint has STARTED, a missing data partner is
  filled with its neutral. Outputs with no painted input are left alone.
- `painted_channel_tile`: the GPU readback through
  `PaintState::channel_target`, which reads and never materializes.
- `assemble_tile_map_sets` takes the per-channel reader, and
  `TileMapSet` carries `painted_channels`.

## Built (`export_dialog.rs`)

- The dialog passes the channel reader, runs the partner fill per tile,
  and filters the preset against maps present on EVERY tile. A channel
  painted on one tile only does not half-write a multi-tile output: it
  is skipped and named, as before.
- The status line appends `[+ painted: roughness, …]`, the maps paint fed
  on every tile, by their `$srcMap` tokens. They are untranslated by
  design, and no catalog key is needed while the catalogs sit under a
  sibling lock.
- The module docs' "Base Color ONLY" paragraph is rewritten to the new
  scope.

## Tests (5 new)

- `flatten_over_keeps_paint_and_fills_the_neutral`: exact bytes for the
  unpainted, opaque and half-alpha (164 = the derived midpoint) texels,
  and the wrong-length `None`.
- `painted_channels_land_only_where_painted`: a painted roughness lands,
  an empty normal exports flat, metallic is not invented, and baked AO
  is untouched.
- `partners_fill_only_outputs_paint_started`: nothing painted fills
  nothing; painted roughness fills the dielectric metallic; a painted
  map is never overwritten.
- `painted_data_channels_are_the_standard_set_names`: every default
  data channel has an export path. This test caught `emissive` being
  outside the default set on its first run, and the doc now names that.
- **`painted_roughness_reaches_the_exported_orm_png`** (GPU, ran on the
  build host with no skip): paints the roughness channel white, runs the
  real glTF preset and decodes the packed PNG. Its B slot holds both 255
  (paint) and 128 (the neutral). **Mutation check:** forcing the channel
  bridge to `None` fails it at the `painted_channels` assert.
- Gate 1 (umber-app): fmt, clippy, 468 tests.

## Judgment calls

- **Neutrals are fixed, not per-material.** An OpenPBR material's own
  roughness value would be a truer "unpainted" fill, but it is not on
  the export path today. The fixed neutrals are documented, and the
  per-material version is the follow-up.
- **AO stays baked.** A painted AO channel is not in the table. The bake
  is the honest source, and painting over it is a layer job.

## Deferred

- Per-material neutrals (OpenPBR defaults) for the unpainted fill.
- A translated "painted:" status phrase (after the catalog lock).
