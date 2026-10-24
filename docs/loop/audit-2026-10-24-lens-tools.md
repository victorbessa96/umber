# Umber audit — lens A: the TOOLS census (2026-10-24)

Date: 2026-10-24. Author: dragon's solo read-only lens-A claw. Brief:
`.dragon/briefs/2026-10-24-audit-lens-tools.md`. Report target:
`docs/loop/audit-2026-10-24-lens-tools.md` (this file).

## Method

Solo read-only pass. No cargo, no builds, no file edits outside this
report. Every claim below was re-derived by grep against the live tree at
the 2026-10-24 head; a claim that could not be tied to a `file:line` was
dropped rather than guessed. Runtime/GPU behavior is NOT verified here
(name the gap, do not paper over it).

## The headline

The live tree matches the brief's census almost exactly — the four
"surfaces" the brief worried docs might have drifted on are all present
and wired. Concretely:

- **The strip** (`crates/umber-app/src/toolbar.rs:148`, `ToolKind`) holds
  exactly the six brief-named tools and nothing more (verified below).
- **The brush tools** (`crates/umber-brush/src/retouch.rs:113`,
  `BrushTool`) hold exactly the brief-named three retouch arms plus the
  GPU-paint default.
- **The stroke methods** (`crates/umber-brush/src/stroke_method.rs`,
  `StrokeMethod`) hold exactly the four brief-named methods.
- **The dynamics** (stamps/jitter/angle/falloff/front-face/hardness/flow/
  size-by-pressure/stabilizers) all have live engine sites, live panel UI
  keys, and headless tests.
- **The shapes** (path, polygon fill, drag-dot, straight-line) all have
  live engine sites and UI.

## The headline table

| # | Surface | Engine site (live) | UI reach | Tests pinning it | Parity |
|---|---|---|---|---|---|
| 1 | Strip tools | `toolbar.rs:148` `ToolKind` (Paint, Smudge, Clone, Erase, Pick, IdMask); `ALL` `:170-177`; `ToolKind::of(&BrushTool)` `:182-189` (`retouch.rs:112-133` for the tool half) | Strip draw `:405-421` (`ToolRow` + `SelectTool`), contextual strip `:533-605`, E→Erase via registry `:861`, hold-E via `TempTools` `:615-739`; Pick/IdMask click handling `viewport.rs:304-316`, `viewport.rs:1481-1496`; arming `main.rs:528,539`, dispatch `main.rs:2174` | `toolbar.rs:853`, `toolbar.rs:1100`, contextual tests `toolbar.rs:745-825`; `brush_panel.rs:2899-2925` | **Complete** (six brief-named strip tools all verified present) |
| 2 | Brush tools (Paint GPU splat / Smudge / Clone / Erase CPU retouch) | GPU splat `umber-gpu/src/paint.rs:1060` (`splat_dabs`); CPU retouch `umber-brush/src/retouch.rs:346` (`smudge_dab`), `:496` (`clone_dab`), `:792` (`erase_dab`), masked variants `:359/:508/:799`, inpaint `:595`; `BrushTool` `retouch.rs:112-133`, `is_retouch` `:127-133`; module docs gate `retouch.rs:18-21` | `brush_panel.rs:373` `tool_rows`, `:1103-1132` `tool_selector_row`; retouch rows `:404-415` (clone-offset/follow, erase-channels), `:715` (strength) | `retouch.rs:1072,1132,1168,1211,1238,1268,1348,1372,1402,1563,1621,1696`; retouch history hook `paint_state.rs:31-74` | **Complete** — brief's retouch-parameter list (smudge strength/smear, clone source offset + follow, eraser channel scoping) all present at the cited sites |
| 3 | Stroke methods | `stroke_method.rs:80-102` (`Space` default, `Airbrush{rate_hz}`, `Dash{ratio,length_px}`, `DragDot`); serde wire `:73-112`; validate `:147-167`; `MethodState` `:171-306`; cap `AIRBRUSH_MAX_DABS_PER_PUSH` `:52` | `brush_panel.rs:1439` `stroke_method_rows`; i18n keys `brush.method-*` (`en-US.ftl:425-433`) | `stroke_method.rs:312-384`, `:386-408`, `:429-466`; `wiring.rs:615,637,651,680,712,731` | **Complete** — the brief's four are all landed; the enum holds exactly those four (no strays) |
| 4 | Brush dynamics | Engine: `dab_adapter.rs` (`BrushParams` `:173-218` incl. `hardness:183-184`, `pressure_gamma:185-187`, `wash_mode:188-192`, `falloff:197-198`, `falloff_custom:203-204`, `stamped:205-210`, `stamp_angle:212-213`, `jitter:214-217`; `WashMode` `:31-39`; `StampAngle` `:44-96`; `Jitter` `:104-139`; `opaque_linearize:297-323`; `wash_scale:330-341`; `stamps_to_dabs_wash:385-401`; `dab_for:254-295`); falloff `falloff.rs:85-202`; front-face `viewport.rs:321-328,1473-1477,925`, tests `viewport.rs:2328,2344`; `main.rs:3934`; panel `brush_panel.rs:585-618,741-744,1220-1257,1280-1285` | Brush panel rows + i18n keys (`brush.hardness`, `brush.pressure-gamma`, `brush.wash-mode`, `brush.falloff*`, `brush.stamped`, `brush.stamp-angle`, `brush.jitter-*`, `brush.front-face-falloff`, `brush.alpha`, `brush.size`, `brush.spacing`, `brush.dabs-per-radius`) | `dab_adapter.rs:419-449,452-510,614-711,739-870`; `falloff.rs:215-403`; `viewport.rs:2328,2344`; `one_euro.rs:205,388,408`; `lazy_mouse.rs:122`; `line_constraint.rs:218,273,298` | **Complete** for everything the brief named; see "surprises" for the two naming merges (size-by-pressure, straight-line) |
| 5 | Stroke shapes | Path: `path_tool.rs:57` (`PaintPathTool`), `:33-49` (`PathOpKind`), `:296-306`; `umber-brush/src/path.rs:163-377` (`PaintPath`, `PaintPath::replay:364`, `PathOp:414`, `PathDab:393`, `PATH_MAX_*:83-94`); `PaintState::replay_path:4268`, `replay_filled_path:4315`; polygon fill: `umber-mesh/src/polygon_fill.rs:265` (`rasterize_polygon_mask`), `MeshData` fill mask at `umber-mesh/src/lib.rs:39`; fill-layer side `fill_layer.rs:468` (`PolygonFillTool`), `:667` (`polygon_fill_rows`), `:629` (`apply_to`) | Path/polygon rows: `path_tool.rs:410-462`; fill rows `fill_layer.rs:967+`; i18n `brush` path keys `en-US.ftl:728-822` | `path_tool.rs:588-711`, `path.rs:479,545`, `fill_layer.rs` polygon tests, `viewport.rs:2205-2225` (`straight_line_snaps_to_five_degree_rays`), `wiring.rs:420,440` | **Complete** — drag-dot, path, polygon, straight-line all landed; straight-line surfaced as a constraint + Shift preview, not a `ToolKind` |
| 6 | Masks | Quick mask: engine `umber-brush/src/quick_mask.rs:122-360` (`QuickMask`, gates `:197`, stamp `:271`, invert `:319`, `MaskPaint:71`); paint-state gate `paint_state.rs:2126-2218`, edit-layer scoping `:2218-2230`; retouch ops masked too `retouch.rs:359,508,799,595`; UI `toolbar.rs:459-478` (X/I), `brush_panel.rs:1143`, `brush_panel.rs:1512` | Quick mask: toolbar X/I, Brush panel mask rows, Layers panel masks section (`en-US.ftl:530-535`); paint-through stencil: `stencil.rs:319` (`quick_mask_gate_bytes`), `umber_core::stencil::forward_map`, viewport overlay (`viewport.rs:112-194,698-750`), set-switch `main.rs:176-178,2542`, panel `main.rs:1200-1203` | `quick_mask.rs:392-481`; cavity `cavity.rs` tests via `brush_panel.rs:1376-1398`; ID-mask `id_mask.rs:200-263` | **Mostly complete** — quick mask + cavity + stencil + ID-mask all landed; see named gaps (quick mask persistence) |
| 7 | Symmetry | Engine: `umber-brush/src/symmetry.rs:48-119` (mirror/radial/`SymmetrySettings`), transforms `:251-277`, `:282-287`, texel conjugates `:310-334`, `expand_dabs:324-334`, coverage union `:343-370`; tests `symmetry.rs:409-633` | Panel: `brush_panel.rs:1572` (`symmetry_rows`); paint state `paint_state.rs:1920` (`sync_symmetry`); fill coverage `fill_layer.rs:294` (`apply_symmetry`); UV-view guides `uv_view.rs:750` | `symmetry.rs:409-633` | **Complete** (engine+UI) — **not persisted** (named gap, below) |
| 8 | Fill / material tools | Engine: fill `fill_layer.rs:77` (`FillLayerState`), `:374` (`refresh_fill_layer`), `:432` (`PolygonFillMode`), `:468` (`PolygonFillTool`), `:819` (`FillLayerStates`); material `umber-paint/src/material_layer.rs:549` (`MaterialLayerState`), `:384` (`apply_material_overrides`), `:315` (`discover_material_outputs`), `:360` (`validate_material_channels`); layers panel `umber-app/src/layers_panel.rs:250` (`material_section`), `:308`, `:359`; anchors `umber-app/src/document.rs:1056-1130`, `main.rs:744-759`; graph anchors `graph_panel.rs:625,1320` | Layers panel sections; stroke gate `layers_panel.rs:79-82` (`paint_gate_allows`: materials/instances carry no paint) | `fill_layer.rs:1266-1860`, `material_layer` tests | **Partial** — fills/polygon paintable; materials composite-only |
| 9 | Wash mode | Engine: `dab_adapter.rs:31-39` (`WashMode`), `BrushParams.wash_mode:188-192`, `stamps_to_dabs_wash:385-401`, `opaque_linearize:297-323`; panel `brush_panel.rs:603-618`; `main.rs:905`; paint state `paint_state.rs:1908` (`sync_wash_mode`) | Brush panel wash row (`brush.wash-mode`, `brush.wash`, `brush.build-up` keys) | `dab_adapter.rs:739-870`, `preset.rs:592` (`wash_mode_round_trips_and_old_files_default_to_build_up`) | **Complete** |
| 10 | Retouch parameters | Engine: `retouch.rs:29-109` (smudge strength/smear + tilt, clone source offset/follow, erase channels); masked ops `retouch.rs:346-359,496-508,792-799,595`; retouch history `paint_state.rs:588-592`, `retouch_writes` `:62-63`, undo `:2667` | `brush_panel.rs:404-415,577,715,1088-1089`; i18n `en-US.ftl:508-510,521` | `retouch.rs:1072,1132,1168,1211,1238,1268,1348,1372,1402,1563,1621,1696` | **Complete** |

## The named gaps

Each gap below is "what exists / what is missing / where the seam sits".

1. **Quick mask: session-only.** Exists: mask state, gates, UI, retouch
   gating (`quick_mask.rs:122-360`, `toolbar.rs:459-478`,
   `retouch.rs:359,508,799,595`). Missing: persistence + undoability.
   Named boundary (`en-US.ftl:522`): "The mask lasts for this session only:
   it is not saved and not undoable." Seam: the session-scoped mask state in
   `PaintState` (`paint_state.rs:2126-2218`, edit-layer scoping `:2218-2230`)
   vs. the mask *design* that expects a saved mask.
2. **Symmetry: session-only.** Exists: mirror + radial engines + panel +
   fill coverage (`symmetry.rs:48-119,251-277,282-287,310-334,343-370`).
   Missing: persistence (not in the `.umber` project nor the
   `.umberbrush` preset) — named in-module at `symmetry.rs:103-106`.
   Seam: `SymmetrySettings` derives `Serialize`/`Deserialize` but the
   module doc withholds persistence.
3. **Materials / instances carry no stroke paint.** Exists: material
   composite engine (`umber-paint/src/material_layer.rs:549,384`),
   layers-panel section, anchors. Missing: they are composite sources, not
   stroke targets. Named boundary: `layers_panel.rs:79-82`
   (`paint_gate_allows` — "materials (like instances) carry no paint").
   Seam: the fill-layer path (`fill_layer.rs`) is the paintable surface;
   mesh-map bakes are the texture-set surface.
4. **Airbrush: backlog dropped, not replayed.** Exists: bank + cap
   (`stroke_method.rs:246-285`, cap `:52`). Missing/named behavior:
   emissions past `AIRBRUSH_MAX_DABS_PER_PUSH` are dropped at
   `:275-284`, never replayed; huge time jumps cannot flood. Intentional
   boundary, documented at `stroke_method.rs:27-31`.
5. **Contextual strip scope.** Exists: strip shows Size/Flow/Spacing/Lazy/
   Symmetry only (`toolbar.rs:483-605`, doc `:483-486`). Missing: hardness,
   jitter, angle, falloff, stamp, method — those live in the Brush panel
   (`brush_panel.rs:585-632,1220-1257,1280-1285`). Seam: two UI surfaces, one
   engine truth (shared `BrushParams`/`SymmetrySettings`).
6. **Retouch undo seam.** Exists: per-stroke retouch history
   (`paint_state.rs:588-592`, `retouch_writes` `:62-63`, `undo_stroke:2667`).
   Named open boundary: split-redo under merged undo entries (design
   follow-up; see `docs/specs/smudge-clone-design.md` via `retouch.rs:1-21`).

## The surprises

1. **The brief's "plus any others in the enum" resolved cleanly**: `StrokeMethod`
   holds exactly `Space`, `Airbrush`, `Dash`, `DragDot` — no strays
   (`stroke_method.rs:80-102`, `RawStrokeMethod:105-112`); the adversarial
   probe `{"kind":"spray"}` is rejected (`stroke_method.rs:419`).
2. **Front-face falloff is fully landed**, contrary to any doc implying it
   was pending: field + setter + pressure-channel wiring
   (`viewport.rs:321-328,1473-1477,925`), shell call `main.rs:3934`, panel key
   `brush_panel.rs:1047`, i18n `en-US.ftl` `.front-face-falloff`, tests
   `viewport.rs:2328,2344`. Default off, verified.
3. **"Size-by-pressure" is not one flag** — it is `pressure_gamma` plus the
   `pressure → alpha` / `pressure → radius` rows (`dab_adapter.rs:185-187`,
   `brush_panel.rs:741-744`, i18n `brush.pressure-alpha` /
   `brush.pressure-radius`, `paint_state.rs:1775` `radius_scale`).
4. **Straight-line is not a `ToolKind`**: it is a `LineConstraintMode`
   (`Off`/`Line`/`AngleSnap`, `line_constraint.rs:41-62`) plus the Shift-held
   dotted preview (`viewport.rs:816-838,1328-1340`, `straight_line_end:1970`)
   plus angle snap (`wiring.rs:440`, tests `viewport.rs:2205-2225`). The
   brief's "(shift-lock?)" is resolved: Shift previews, Ctrl/Cmd snaps.
5. **Pick and IdMask really are strip-only**: no `BrushTool` arm for them
   (`toolbar.rs:182-189`), the panel returns early for them
   (`brush_panel.rs:493`), and the viewport suppresses painting while armed
   (`viewport.rs:1481-1496`) — yet both sit in `ToolKind::ALL` and the strip.
6. **The strip's contextual row is a strict subset** of the Brush panel's
   rows (doc `toolbar.rs:483-486` vs. implementation `:533-605`), so the
   brief's "(d) UI reach (panel/strip/shortcut)" question resolves to both,
   not one.
7. **Wash mode** is complete end-to-end (engine `dab_adapter.rs:31-39`,
   `:385-401`; panel `brush_panel.rs:603-618`; shell `main.rs:905`; paint
   state `paint_state.rs:1908`; tests `dab_adapter.rs:739-870`, `preset.rs:592`)
   — build-up vs. normalized wash, as the brief framed it, both shipped.

## Unverified (needs a runtime / GPU run — not papered over here)

- No GPU/window run this pass; the splat/falloff math is pinned headless,
  but live dabs-on-screen behavior is not.
- egui panel rendering reach (`show` paths) is cited from fn line numbers,
  not from a frame run; headless `egui::Context` tests cover the pure halves.

## Gates run

None this pass (read-only audit: no cargo, no builds). `python3
scripts/check-layers.py` not run (no crate/layer surface touched).

LENS-A-DONE
