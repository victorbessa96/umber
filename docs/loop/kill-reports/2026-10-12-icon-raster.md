# `icon-raster` — the icon atlas build step

Brief: rasterize the lucide ids in `TOOL_ICONS` + `PANEL_ICONS` so every
tool-row glyph draws real pixels instead of the placeholder box.
Contract: `crates/umber-gui/src/icons.rs` module docs (the raster step's
contract), `AtlasBuilder::push` as the raster output's consumer.

## Pinned lucide version

**1.55.0** — fetched 2026-10-12 from
`https://raw.githubusercontent.com/lucide-icons/lucide/1.55.0/icons/<name>.svg`
(release page `https://github.com/lucide-icons/lucide/releases/tag/1.55.0`).
Recorded in three places: `tools/icon-raster` `SOURCE_URL`,
`umber_gui::icons::LUCIDE_VERSION`, and the generated `manifest.json`
(`lucide` + `source` fields — the loader refuses a manifest whose pin
differs from the code's).

Exact fetch commands (reproducible without this agent):

```sh
VER=1.55.0
for n in paintbrush eraser fingerprint stamp pipette paint-bucket spline \
         pentagon square-dashed flip-horizontal-2 file folder-open save \
         pencil undo-2 redo-2 eye panel-left; do
  curl -fSL "https://raw.githubusercontent.com/lucide-icons/lucide/$VER/icons/$n.svg" \
    -o "assets/icons/lucide/$n.svg"
done
```

## Missing ids (findings, NOT substitutions)

2 of 18 ids are absent from 1.55.0 — both renamed upstream, verified on
lucide.dev (`/icons/fingerprint` and `/icons/flip-horizontal-2` resolve to
the successor pages):

| Declared id | Status in 1.55.0 | Successor (not adopted) |
|---|---|---|
| `fingerprint` (smudge) | renamed | `fingerprint-pattern` |
| `flip-horizontal-2` (symmetry) | renamed | `triangles-centerline-dashed-horizontal` |

Per the brief these are findings: the SVGs were not fetched, the tool
records both in the manifest's `missing` array (and warns on stderr), the
loader test pins that exact list, and both slots keep the placeholder box.
Adopting the successor ids means editing `TOOL_ICONS` — a design call for
the follow-up, deliberately not snuck in here.

## Atlas layout (as generated)

Uniform cells, declaration order minus missing, `pack_grid` math:

| Section | Cell | Icons | Grid | Atlas px | File |
|---|---|---|---|---|---|
| tool | 20 | 8 | 3 cols x 3 rows | 60x60 | `assets/icons/atlas/tool.png` |
| tool2x | 40 | 8 | 3 cols x 3 rows | 120x120 | `assets/icons/atlas/tool@2x.png` |
| panel | 16 | 8 | 3 cols x 3 rows | 48x48 | `assets/icons/atlas/panel.png` |
| panel2x | 32 | 8 | 3 cols x 3 rows | 96x96 | `assets/icons/atlas/panel@2x.png` |

Tool order: paintbrush, eraser, stamp, pipette, paint-bucket, spline,
pentagon, square-dashed. Panel order: file, folder-open, save, pencil,
undo-2, redo-2, eye, panel-left. `manifest.json` is pre-seeded by hand in
the canonical writer format (`serde_json::to_string_pretty` + `\n`,
`BTreeMap` icon order); the dragon's tool run rewrites it — byte-identical
if the pre-seed matches, cosmetic whitespace drift otherwise. The four PNGs
do NOT exist in the tree yet (nothing here can execute the rasterizer);
they appear when the dragon runs the tool at gate time.

## The cycle decision (and the second, quieter reason)

`umber-gui` does NOT depend on `umber-export`. No cycle exists either way
(only `umber-app` depends on `umber-gui`), but the reuse was rejected on
two independent grounds, both documented in `icons.rs`' module docs:

1. Layering — `umber-gui` depends on egui alone; `umber-export` would drag
   `umber-graph` + the image codec stack into the presentation crate.
2. Semantics — `umber-export::png::read_png_rgba8` flips to GPU row order
   for equirect sampling; the atlas needs top-first rows for `ColorImage`.
   Reuse would flip every glyph vertically.

So the loader carries its own 8-bit-RGBA-only decoder (`decode_atlas_png`,
top-first, no flip — with a contrast test against the equirect flip) on the
`png` micro-crate (0.17, same as `umber-export`), plus `serde_json` for the
manifest. `umber-gui` gains no workspace-internal dependencies.

Single-copy assets: the PNGs + manifest live once under
`assets/icons/atlas/`; the `embedded-icons` feature bakes them via
`include_bytes!`/`include_str!` with `CARGO_MANIFEST_DIR`-relative paths
(the `fonts.rs` `embedded-fonts` precedent) instead of duplicating files
into `crates/umber-gui/assets/`. Runtime loads from the same dir (override:
`UMBER_ICON_ATLAS_DIR`). Missing files are `AtlasLoadError`, never panic.

## Judgment calls (also in module docs)

- `currentColor` -> white before parsing (no CSS at build time; the atlas
  convention is white glyphs tinted at draw time).
- tiny-skia renders premultiplied; `AtlasBuilder::push` wants unmultiplied
  -> halved-rounding un-premultiplication (`(c*255 + a/2)/a`), exact-value
  tested.
- Missing SVGs skip with a warning + manifest record, never substituted;
  unparseable SVGs and all-zero-alpha renders are hard errors.
- The tool packs with `umber-gui`'s own `pack_grid` (path dependency on
  `umber-gui`), so raster output cannot drift from loader expectations;
  uniform cells make the loader's `AtlasBuilder` repack an identity.
- Loader version-gates schema AND lucide pin (a re-pin without re-raster
  refuses to load).
- Non-square SVG canvas is a hard error (the packer needs squares).

## Deferred

- App wiring: boot still constructs `IconAtlas::empty()` (`umber-app`
  `main.rs:603`, untouched — out of surface). Swapping in
  `load_baked_atlas` is the follow-up; requirements row marked
  NOT REACHABLE accordingly.
- Runtime re-raster on DPI change (the 2x bake covers HiDPI; callers pick
  the scale once). Recorded in `icons.rs` module docs.
- `TOOL_ICONS` successor ids (`fingerprint-pattern`,
  `triangles-centerline-dashed-horizontal`) — see missing-ids table.
- `embedded-icons` feature code is written but uncompiled (feature off
  until PNGs land): the dragon should build once with
  `--features embedded-icons` after generating the PNGs.

## Unverified (no bash, no cargo in this session)

- resvg/usvg/tiny-skia call shapes beyond the vendored excerpts
  (`Tree::from_data`, `Options::default`, `resvg::render`, `Pixmap::new /
  as_mut / take`, `Transform::from_scale`, `tree.size().width/height`):
  verified against `docs/loop/artifacts/resvg-api-refs/` where vendored;
  `Transform::from_scale` and `Size::width/height` are long-stable but not
  in the excerpts — the gate compile is the check.
- `png` 0.17 `Decoder::new(&[u8])` + `output_buffer_size` + `next_frame`
  shapes mirror `umber-export/src/png.rs` in-tree usage exactly.
- wgpu descriptor shapes in the `gpu` test mirror
  `umber-gpu/src/texture_display.rs:561-587` verbatim.
- `egui::Context::default()` + `load_texture` headless use (standard egui;
  no renderer involved).
- The tool's offline resolution (`resvg = "=0.44.0"` + full default
  features incl. fontdb): the registry cache holds the crates per the
  brief; the gate build confirms the closure.
- `cargo fmt --all`, `cargo clippy -p umber-gui --all-targets -- -D
  warnings` (also `--features gpu`, `--features embedded-icons` once PNGs
  land), `scripts/verify-kill.sh`: NOT run (no bash) — dragon runs all at
  gate time.

## Files written

- `assets/icons/lucide/*.svg` (16 files; the 2 missing ids absent — finding)
- `assets/icons/atlas/manifest.json` (pre-seed; tool rewrites at gate)
- `tools/icon-raster/Cargo.toml` + `tools/icon-raster/src/main.rs`
  (detached `[workspace]`; 9 tests)
- `crates/umber-gui/src/icons.rs` (loader + 11 tests, 1 `gpu`-gated)
- `crates/umber-gui/Cargo.toml` (`png`, `serde_json`, `pollster`/`wgpu`
  dev-deps, `embedded-icons` + `gpu` features)
- `crates/umber-gui/src/lib.rs` (re-exports)
- `docs/specs/requirements.md` (§11 theme row: icon-raster clause)
- this report

## Gates run

- `cargo fmt` / `cargo clippy` / `scripts/verify-kill.sh`: NOT RUN —
  sandbox refuses bash (expected wall, named in the brief). Dragon runs at
  gate time. Code was written against the repo's trap list by hand
  (matches! arity, no struct-in-impl, no format-string field access,
  doc-attachment, neg-cmp, field shorthand).
