# Wave 3.1 research — .abr patt pattern sections: analysis + kill plan

Landed 2026-11-09 by Razul from a claude-fable-5.2 planning pass (claude CLI 2.1.318, headless -p; raw result in session scratch). Planning only; no repo code touched by the pass. All file:line references are against the tree at e078614e/4d6bc5a3.

Findings that matter before the body:

Two findings matter before you read it:
- **Two importers exist.** The CLI uses the clean-room `umber-brush` reader, but the GUI brush panel calls the `ag-psd` crate directly (`crates/umber-app/src/brush_panel/abr_import.rs:109`). Work on `abr.rs` reaches the CLI only.
- **3.1 as scoped does not change how a brush looks.** Reader, mapping and sidecar carry the pattern; no painted pixel changes until the paint engine samples it, which is a separate kill on a surface the wave plan does not list.

---

# Wave 3.1 — `.abr` `patt` pattern sections: analysis and kill plan

Planning only, 2026-11-09. All `file:line` references are against the tree at `e078614e`.

## 0. Summary

- A `patt` section is a list of **tiling pattern images** (RGB, grayscale or indexed, no alpha). In a brush file its job is to be the **brush's Texture**: a brush descriptor with `useTexture = true` names a pattern by UUID (`Txtr.Idnt`), and the pattern is tiled across the canvas and blended into every dab.
- **Recommended target: a brush texture**, carried as a `<pattern-slug>.pattern.png` file beside the presets plus an optional `texture` block in the `.umberbrush`. It is not a stamp, a stencil or a fill source.
- **The wave plan overstates what 3.1 delivers.** `2026-11-09-wave-plan.md:95-97` says a brush without its pattern "does not look like itself". That only becomes true once the paint engine samples the texture (kill 3.1c below).
- Three kills, disjoint surfaces: **3.1a** reader + mapping (umber-brush only), **3.1b** preset schema + CLI write, **3.1c** engine consumer (needs its own design brief).

## 1. What a `patt` section contains

### Provenance of the format knowledge

Format knowledge here comes from public analysis of the format, not from any vendor tool or SDK:

1. The vendored MIT `ag-psd` reference, already this module's declared format authority (`crates/umber-brush/src/abr.rs:16-23`): the section walk at `docs/claw-artifacts/ag-psd/src-tree/src/abr.rs:1024-1029` and the pattern record reader `read_pattern` at `docs/claw-artifacts/ag-psd/src-tree/src/reader.rs:3013-3264`.
2. The upstream test corpus's expected values, already vendored: `crates/umber-brush/tests/fixtures/abr_real_v6_2_samp_desc.data.json` carries a `patterns` entry and a `texture` entry for a real file.
3. The publicly documented PSD-family "Patterns" / "Virtual Memory Array List" layout, which the reference mirrors. I cite it from memory and did not re-fetch it; where the two disagree the reference wins, per the module's own rule (`abr.rs:22-23`).

As with rows 268/282, the kill pattern-ports the *logic* only. No reference code, comments or prose are copied.

### Section level

`8BIM` + `patt` + u32 BE size, then pattern records back to back until the section end (`ag-psd abr.rs:1024-1029`). The section is padded to 4 bytes like every other section. A `patt` section may be empty: the vendored whole-file fixture `abr_real_v6_2_full.abr` has `8BIMpatt` at offset 16 and `8BIMdesc` at offset 28, so its `patt` size is 0.

### Record level

All fields big-endian.

| Field | Type | Notes |
|---|---|---|
| record length | u32 | Excludes itself. The record is padded to a 4-byte boundary (`reader.rs:3014-3018`, `3249`). |
| version | u32 | Must be 1 (`reader.rs:3019-3025`). |
| image mode | u32 | 1 = grayscale, 2 = indexed, 3 = RGB. The reference refuses everything else (`reader.rs:3027-3040`, table at `819-831`). |
| point | i16, i16 | The pattern's size as **(vertical, horizontal)**. The reference names these `x`, `y` in read order, which is swapped: the real file's oracle has `x: 597, y: 595` for a `w: 595, h: 597` pattern. Redundant with the rectangle; do not use it for sizing. |
| name | u32 count + UTF-16BE code units | A trailing NUL unit is dropped (`reader.rs:554-580`). Real value: `"Bobbys Paint"`. |
| id | Pascal string, 1-byte length, no padding | UUID-ish, Latin-1. Real value: `d667d14d-41d6-894d-b613-58c58768aab0`. This is the link key. |
| palette | 256 × 3 bytes RGB, then 4 skipped bytes | Indexed mode only (`reader.rs:3046-3056`). |
| pixel-array version | u32 | Must be 3 (`reader.rs:3058-3064`). |
| pixel-array length | u32 | Read and ignored (`reader.rs:3066`). |
| rectangle | 4 × u32: top, left, bottom, right | Width = right − left, height = bottom − top (`reader.rs:3071-3086`). |
| channel count | u32 | The loop runs `count + 2` slots (`reader.rs:3101`); the two extras are mask slots in the public layout. |

Each channel slot (`reader.rs:3102-3115`):

| Field | Type | Notes |
|---|---|---|
| written flag | u32 | 0 means the slot is empty and nothing else follows for it. |
| length | u32 | Covers everything after itself: `data_len = length − 23` (4 depth + 16 rect + 2 depth + 1 compression). |
| pixel depth | u32 | 8, 16 or 32 are decodable (`helpers.rs:484-491`). |
| rectangle | 4 × u32 | The channel's own rect; may be a sub-rect of the pattern rect, landing at offset `(cleft − left, ctop − top)` (`reader.rs:3154-3155`). |
| pixel depth again | u16 | Must equal the u32 depth (`reader.rs:3119-3124`). |
| compression | u8 | 0 = raw, 1 = PackBits rows with a u16-per-row length table first (`reader.rs:3159`, `3210-3226`, `2389-2393`). The public prose describes mode 1 as zip; the reference uses PackBits and wins. |
| data | `length − 23` bytes | Big-endian samples; 16-bit keeps the high byte, 32-bit is a float mapped to 8 bits (`reader.rs:2114-2123`). |

How written slots map to pixels (`reader.rs:3159-3246`): a counter of *written* slots picks the destination.
- RGB takes written slots 0, 1, 2 as R, G, B.
- Grayscale takes written slot 0 and replicates it to R, G, B.
- Indexed takes palette lookups, raw only (indexed + RLE is refused at `reader.rs:3233-3237`).
- Later written slots (the masks) are ignored. Output is RGBA8 with alpha fixed at 255 (`reader.rs:3091-3096`).

### How a brush refers to a pattern

The link lives in the `desc` section, not in `patt` (`ag-psd abr.rs:740-753`):

- `useTexture` (bool) gates everything.
- `Txtr` is a descriptor with `Idnt` (the pattern id) and `Nm  ` (its name).
- On the brush descriptor itself: `textureScale` (percent), `textureDepth` (percent), `textureBlendMode` (enum), `InvT` (invert), `textureBrightness`, `textureContrast`, `TxtC` (texture each tip), plus depth dynamics.

Real values from the vendored oracle: scale 0.66, depth 1, blend mode "color burn", invert false, each-tip true.

### How `patt` differs from `samp`

| | `samp` | `patt` |
|---|---|---|
| What it is | A brush tip: one coverage bitmap | A tiling image |
| Channels | 1 (alpha) | 1 gray, 3 RGB, or indexed + palette; no alpha |
| Text | Pascal id only | UTF-16 name + Pascal id |
| Parameter block | 10 or 264 skipped bytes by minor version (`abr.rs:412`) | None; layout does not depend on the minor version |
| Bounds | 4 × i32, absolute position | 4 × u32 pattern rect, plus a rect per channel |
| Depth / compression | i16 depth, u8 compression, once | u32 + u16 depth, u8 compression, per channel |
| 16-bit RLE | Refused (`abr.rs:690-692`) | Decodable in the reference |
| Linked from | the tip shape's `sampledData` | the brush's `Txtr.Idnt`, gated by `useTexture` |
| Sharing | usually one tip per brush | commonly one pattern for many brushes |
| Typical size | kilobytes | up to megabytes (the real fixture's is ~1 MiB, `FIXTURES.md:19,70`) |

## 2. Current tree state

### Clean-room reader (`crates/umber-brush/src/abr.rs`)

| What | Where |
|---|---|
| Public entries | `read_samples` `abr.rs:362`, `read_abr` `abr.rs:385` |
| The one function that parses section headers | `walk` `abr.rs:390-463` |
| Header fields | signature `abr.rs:420-423`, kind `abr.rs:424`, size `abr.rs:425-427` |
| Descriptor-section gate (Full mode, `desc`/`phry` only) | `abr.rs:428-442` |
| Payload slice (also bounds-checks the declared size) | `abr.rs:443` |
| Dispatch | `samp` `abr.rs:445-446`, `desc` `abr.rs:447-448`, `phry` `abr.rs:449-453` |
| **Where `patt` is ignored** | It matches no arm of the `if` chain at `abr.rs:445-453`, so the payload taken at `abr.rs:443` is dropped. There is no explicit `patt` arm. |
| 4-byte section padding | `abr.rs:455-459` |
| `samp` record parsing | `parse_samp` `abr.rs:562`, `parse_samp_with_budget` `abr.rs:572`, `parse_sample` `abr.rs:620` |
| PackBits (reusable for 8-bit pattern rows) | `decode_rle` `abr.rs:710`, `decode_packbits_row` `abr.rs:733` |
| Cursor (private) | `Reader` `abr.rs:786-851` |
| Result type | `AbrFile` `abr.rs:316-324`: no patterns |
| Brush type | `AbrBrush` `abr.rs:286-312`: no texture field; `parse_brush` `abr.rs:506-549` never reads `useTexture` / `Txtr` |
| Stated non-goal | module docs `abr.rs:42-43`, `abr.rs:47`; `read_abr` docs `abr.rs:371` |

- **Mapping** (`crates/umber-brush/src/abr_import.rs`): non-goal at `:35`; `map_file` `:80-127`; `map_preset` `:129-184`; the duplicate-stamp drop at `:111-115`.
- **Registration:** `crates/umber-brush/src/lib.rs:31-33`.
- **Existing coverage of the skip:** `abr_real_desc.rs:25-28` reads the whole-file fixture (empty `patt`) and pins 0 samples / 1 brush. No test in `abr.rs` exercises a non-empty `patt` (test list at `abr.rs:1168-1778`).

### CLI consumer (`crates/umber-cli/src/import_asset.rs`)

`import_abr_asset` (`:501-604`) calls `umber_brush::abr_import::import_abr` (`:531`), plans all targets (`:535-567`), refuses the whole import if any target exists (`:565-567`), and writes presets plus `.stamp.png` (`:590-602`). `AbrImportOutcome` is at `:402-423`. The agent step builds its reply JSON field by field at `crates/umber-cli/src/agent.rs:2181-2182`.

### GUI consumer: a second, separate importer

`crates/umber-app/src/brush_panel/abr_import.rs` does not use the clean-room reader. It calls `ag_psd::abr::read_abr` (`:109`) on the `ag-psd = "0.3.0"` dependency (`crates/umber-app/Cargo.toml:46`), wired from `brush_panel.rs:896-909`.

- ag-psd already decodes the patterns on this path; `map_abr(&abr.brushes, &abr.samples)` at `:113` drops `abr.patterns`.
- Texture is reported as "not carried over" at `:280-282`.
- It has its own `patt` pre-flight against hostile allocations at `:155-220`.
- Its mapping differs from the clean-room one: size goes to `preset.size = diameter / 48` (`:364-366`) rather than `lazy_mouse.radius_px` (umber-brush `abr_import.rs:171-174`).

The ROADMAP row (`docs/ROADMAP.md:238`) calls the clean-room reader "complete end-to-end"; that holds for the CLI only.

### Live kill locks

Not checked: `/tmp/umber-kill-locks/` is outside this session's readable directories. Run `bash scripts/kill-locks.sh check <files>` before dispatch.

## 3. Where a pattern should land

### The candidates

**Brush stamp: no.** A stamp is one non-tiling coverage bitmap sampled in the dab's own rotated frame (`crates/umber-gpu/src/shaders.rs:683-689`), one per paint session (`PaintState::set_stamp`, called from `crates/umber-app/src/brush_panel.rs:371`), keyed on `BrushParams::stamped` (`crates/umber-brush/src/dab_adapter.rs:213-218`). A pattern is anchored to the canvas and tiles across dabs. A textured sampled brush also already owns the stamp slot.

**Stencil: no.** The stencil is a binary gate: `StencilImage::from_alpha` thresholds alpha ≥ 128 to 255 (`crates/umber-core/src/stencil.rs:118-138`). A decoded pattern has alpha 255 everywhere, so it would load as a fully open stencil. The stencil is also non-tiling, screen-space, session-global, and has no library format (`crates/umber-app/src/stencil.rs:1-6`, `:40-51`).

**Fill pattern: structurally plausible, wrong purpose.** `FillSource::GraphImage` (`crates/umber-mesh/src/fill.rs:138-143`) is an RGBA image sampled bilinearly with wrap (`fill.rs:396-412`), with five projections (`fill.rs:84-106`). A pattern fits it without loss. Against it:
- The file says "brush X is textured by pattern P"; a fill layer discards that link.
- There is no fill-pattern library kind to import into (`AssetKind`, `import_asset.rs:75-81`), and `GraphImage` pixels are embedded in the project, so an import would have to create a layer in an open document.

It stays available as a later convenience: the sidecar PNG proposed below is what such a feature would read.

### Recommendation: brush texture

1. **Reader:** `AbrFile::patterns: Vec<AbrPattern>` (id, name, width, height, RGBA8) and `AbrBrush::texture: Option<AbrTexture>` (pattern id, name, scale, depth, invert, blend mode as the stored enum string, each-tip).
2. **On disk:** one `<pattern-slug>.pattern.png` per *referenced pattern* (not per brush), beside the presets, and an optional block in the `.umberbrush`:
   ```json
   "texture": { "pattern": "bobbys-paint.pattern.png", "scale": 0.66, "depth": 1.0, "invert": false, "blend": "color-burn" }
   ```
   One file per pattern because packs routinely share one paper texture across dozens of brushes. `sorted_preset_files` only lists `.umberbrush` (`crates/umber-brush/src/preset_library.rs:203`), so the new PNG does not disturb the shelf.
3. **Engine (3.1c):** the paint compute pass already works in target-texel space (`shaders.rs:679-681`), so "anchored to the canvas" becomes "anchored to the target texels": multiply the dab's coverage by the tiled pattern, mixed by depth. Pattern pixels live in the paint session like the stamp, so `BrushParams` stays `Copy`; the block belongs on `BrushPreset`.

Why this target:
- It is the only mapping that preserves the meaning the file encodes.
- It follows the existing stamp sidecar convention (`<stem>.stamp.png`, `brush_panel.rs:350-359`).
- It is additive: brushes without `useTexture` produce byte-identical output.
- The decoded PNG is reusable by a future fill or stencil feature without re-importing.

Decode to RGBA8 even though alpha is constant: it matches the reference's output shape (so an upstream oracle image can be compared byte for byte) and `write_png` takes RGBA (`import_asset.rs:596`).

## 4. Kill plan

### 3.1a `abr-patt-reader` (umber-brush only)

**Surface (lock all of these):**
- `crates/umber-brush/src/abr_patt.rs` — NEW: the pattern record decoder and `AbrPattern`.
- `crates/umber-brush/src/lib.rs` — one line, `pub(crate) mod abr_patt;` next to `lib.rs:32` (rule 13).
- `crates/umber-brush/src/abr.rs` — the `patt` arm in `walk`, `AbrFile::patterns` + `skipped_patterns`, `AbrBrush::texture`, `match_pattern`, texture keys in `parse_brush`, `pub(crate)` on `Reader` / `decode_packbits_row`, module docs.
- `crates/umber-brush/src/abr_import.rs` — required even for a reader-only change: its test helpers build `AbrBrush` and `AbrFile` as full struct literals (`:192-205`, `:224-228`). Also the mapping: `AbrImportEntry::texture`, `AbrImport::patterns`.
- `crates/umber-brush/tests/abr_real_patt.rs` — NEW.
- `crates/umber-brush/tests/fixtures/FIXTURES.md` and, if Q3 says yes, one new fixture.

No other crate needs to change to keep compiling: `AbrImportEntry` and `AbrImport` are only built inside `abr_import.rs` (`:91`, `:120`), and the CLI reads them by field (`import_asset.rs:538-551`, `:578-580`). CLI behaviour is unchanged after this kill.

**Design points to state in the module docs (rule 8):**
- **Pattern decoding is soft.** A `patt` section has been accepted by skipping since row 268, so no pattern problem may turn a file that imports today into a refusal. An undecodable record is skipped by its declared length and counted in `AbrFile::skipped_patterns`; a record running past the section ends pattern parsing for that section and counts once. This is deliberately unlike `desc`, which is strict.
- **Full mode only.** `read_samples` keeps skipping `patt` by length (`abr.rs:337-339`), same reasoning as `abr.rs:74-80`.
- **Caps, checked before allocation:** a per-side cap (see Q5), a patterns-per-file cap, name length ≤ record length, and decoded bytes charged against the same per-file budget as stamps (`MAX_TOTAL_ALPHA_BYTES`, `abr.rs:131`). The GUI pre-flight (`brush_panel/abr_import.rs:155-220`) lists the hostile shapes worth mirroring.
- **No panics.** The reference's `color_mode.unwrap()` (`reader.rs:3041`) and slice indexing must not be ported as such.
- **Texture gate:** `AbrBrush::texture` is `Some` only when `useTexture` is true and `Txtr` is a descriptor (`ag-psd abr.rs:740-741`). Percent fields go through `parse_percent` (`abr_desc.rs:288`); the blend mode through `as_text`, which already reads enums (`abr_desc.rs:276-281`).
- **Duplicate-stamp rule unchanged** (`abr_import.rs:111-115`); see Q4.

**Unit pins on synthetic records (rule 5):**

1. Grayscale 2×2 raw, data `[10, 20, 30, 40]` → RGBA `[10,10,10,255, 20,20,20,255, 30,30,30,255, 40,40,40,255]`; slot length field = 23 + 4 = 27.
2. RGB 2×1 raw, R `[1,2]`, G `[3,4]`, B `[5,6]` → `[1,3,5,255, 2,4,6,255]`.
3. Indexed 2×1 raw, palette[7] = (9,8,7), palette[0] = (1,2,3), indices `[7,0]` → `[9,8,7,255, 1,2,3,255]`; also proves the 768 + 4 palette skip.
4. Grayscale 3×1 RLE: row table `[0x0002]`, row bytes `[0xFE, 0x55]` (257 − 254 = 3 repeats) → three `0x55` gray pixels.
5. 16-bit raw grayscale 1×1, sample `0xABCD` → `0xAB`.
6. Channel sub-rect: pattern rect (0,0,2,2), channel rect (1,1,2,2), value 99 → only pixel index 3 is `[99,99,99,255]`, the rest `[0,0,0,255]`.
7. An RGB record with an unwritten slot between written ones still maps written slots 0/1/2 to R/G/B.
8. UTF-16 name with a trailing NUL decodes without it; a non-ASCII unit round-trips; the id decodes as Latin-1.
9. Two records in one section, the first needing 1–3 pad bytes: both found, in order.
10. Soft failures, one test each: mode 4, version 2, pixel-array version 2, depth mismatch (8 / 16), depth 1, compression 2, indexed + RLE. Each gives `patterns.len() == 0`, `skipped_patterns == 1`, `Ok`, and brushes from a following `desc` still parse.
11. A record length past the section end: `Ok`, counted, samples/brushes intact.
12. A rect of cap + 1 is skipped before allocating (shape of `abr.rs:1285`).
13. `read_samples` on a file with a garbage `patt` payload returns the same samples as without it.
14. `parse_brush`: `useTexture` true + `Txtr{Idnt, Nm}` + scale 66% + depth 100% + `InvT` false → exact field values; `useTexture` false with a `Txtr` present → `None`.
15. `match_pattern` finds by id; an unknown id → `None`.
16. Mapping: a textured brush points at the right pattern index; two brushes sharing one pattern id yield one pattern entry; an unreferenced pattern is counted as an orphan; a brush whose pattern id is missing gets `texture: None` and is counted.

**Real-file pins in `tests/abr_real_patt.rs`:**
- `abr_real_v6_2_full.abr` (already vendored, empty `patt`): `patterns.is_empty()`, `skipped_patterns == 0`, brush still "Soft Round".
- If Q3 allows the fixture, the untrimmed `sample-and-pattern/src.abr`:
  - exactly 1 pattern, id `d667d14d-41d6-894d-b613-58c58768aab0`, name `Bobbys Paint`, 595 × 597;
  - `rgba.len() == 595 × 597 × 4 = 1_420_860`;
  - an FNV-1a checksum of the plane pinned against the upstream oracle image (method of `abr_real_file.rs:53-65`);
  - the brush's `texture.pattern_id` equal to that id, scale 0.66.

### 3.1b `abr-patt-preset-cli` (schema + write)

Dispatch after 3.1a lands.

**Surface:**
- `crates/umber-brush/src/preset.rs` — `BrushPreset::texture: Option<BrushTexture>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`, following the `catalog` precedent (`preset.rs:266-271`); `validate()` ranges.
- Every `BrushPreset` struct literal, since the struct has no `Default`: `abr_import.rs:162` (umber-brush), `preset.rs:431`, `preset_library.rs:264`, `crates/umber-app/src/brush_panel.rs:2599`, `crates/umber-app/src/brush_panel/abr_import.rs:301`, and check `crates/umber-app/src/asset_thumbs.rs:392`.
- `crates/umber-cli/src/import_asset.rs` — pattern slugs, `.pattern.png` targets in the plan and the `TargetExists` check, the write, new outcome counters.
- `crates/umber-cli/src/agent.rs` — reply JSON (`:2181-2182`) and its test (`:3488-3522`).
- `crates/umber-cli/src/main.rs` — usage text (`:64`, `:90`).

This crosses three crates and touches `brush_panel.rs`. The literal sites must land in the same commit as the field or the workspace does not build.

**Pins:**
- A preset without texture serializes with no `"texture"` key, and a pre-texture file loads with `None` (shape of `preset.rs:791-803`); a textured preset round-trips.
- The CLI fixture (`abr_fixture`, `import_asset.rs:613`) extended with one grayscale pattern shared by two brushes yields exactly one `.pattern.png`, both presets naming it.
- A pre-existing `.pattern.png` refuses the whole import and writes nothing.
- Dry-run counts equal real counts.
- The written PNG decodes to the pinned RGBA.

**Rule 6 note:** after 3.1b the `texture` block is persisted but the engine does not read it. That is deliberate and should be stated in the module docs, with 3.1c named as the consumer. The reason to persist now: the import is one-shot, and the whole-import `TargetExists` refusal (`import_asset.rs:565-567`) makes "re-import later to pick up the texture" a manual cleanup job. See Q1.

### 3.1c `brush-texture-engine` (not dispatchable yet)

Needs its own design brief.

Likely surface:
- the paint compute shader and compositor in `umber-gpu` (`shaders.rs:650-700`; `paint.rs:928-937` as the `set_stamp` precedent);
- `umber-paint`'s session;
- `brush_panel.rs` (load the pattern beside the stamp, `:350-371`);
- both `.ftl` catalogs.

Decisions it has to make:
- anchoring space;
- behaviour across UDIM tiles and UV seams;
- which blend modes exist;
- each-tip vs per-stroke;
- a live-render check.

### Byte-compat notes for existing `.abr` imports

- **No `patt`, empty `patt`, or patterns no brush uses:** identical presets, stamps, target list and order. Only new zero-valued or orphan-count fields in the outcome JSON (additive; `agent.rs:3521-3522` and `import_asset.rs:1219-1220` are unaffected).
- **Files that import today must still import**, hence soft decoding. A section size running past the file is already `Malformed` (`abr.rs:443`) and stays so.
- **`read_samples` is untouched**, including on hostile `patt` content (pin 13).
- **Brushes with `useTexture`:** after 3.1b their `.umberbrush` gains a `texture` key and the import writes an extra `.pattern.png`. My grep found no `deny_unknown_fields` on `BrushPreset` (`preset.rs:234`), so older builds should ignore the key; confirm with a test.
- **Target order:** keep "preset, then its stamp" (`import_asset.rs:406-408`) and append pattern files after all preset/stamp pairs.
- **Memory:** `read_abr` will now allocate decoded patterns (1.4 MB for the real one). A pattern that would exceed the shared budget is skipped, not fatal.
- **Duplicate-stamp drop stays as is in 3.1a**; changing it would alter the preset set of files that import today.

### Open questions for the partner

- **Q1. Persist texture parameters before the engine can use them?** Recommendation: yes (3.1b), for the re-import reason above. The alternative is to stop at 3.1a until 3.1c is designed.
- **Q2. Two importers.** Should the GUI move onto the clean-room reader, stay on ag-psd and get its own small pattern kill (ag-psd already hands it `abr.patterns`), or keep reporting "texture not carried over"? Their size mappings also disagree. Decide before 3.1b, which already has to touch `brush_panel/abr_import.rs:301`.
- **Q3. A ~1 MiB real fixture.** The only real file with a non-empty `patt` is upstream `sample-and-pattern/src.abr`; rows 271/282 trimmed its `patt` out (`FIXTURES.md:17-20`, `:68-71`), and a pattern cannot be trimmed without re-encoding. Vendor the whole file (recommended: row 271 showed synthetic fixtures agreeing with the reader's own misreadings), or keep synthetic fixtures and have the dragon run a one-off oracle comparison outside the repo.
- **Q4. Same tip, different texture.** Such brushes are currently collapsed to the first (`abr_import.rs:111-115`). Keep collapsing (recommended for 3.1a), or compare stamp + texture and emit more presets? Revisit in 3.1c.
- **Q5. Size cap.** Stamps cap at 4096 per side; patterns can be larger. Skip oversize patterns (recommended) or downscale?
- **Q6. Blend modes.** The real file uses colour burn. Is a multiply-only v1 engine acceptable, with the stored mode preserved in the preset?
- **Q7. Wave-plan wording.** `wave-plan.md:93-97` scopes 3.1 to `abr.rs` + `abr_import.rs` and promises the look. Reword the row to "carry the pattern", or add 3.1c to the plan.

### Unverified

- The public PSD-format prose (mask slots, "zip" for compression 1, "skip if length is 0") is cited from memory. The reference reads the depth and rect even when a written slot's length is 0 (`reader.rs:3106-3115`); no fixture in the tree settles which is right. Follow the reference and say so in the module docs.
- I could not run a byte-level walk of the fixtures (script execution was not permitted). Section offsets come from a text search for `8BIM` tags; the ~1 MiB `patt` size and the pattern's values come from `FIXTURES.md` and the vendored `data.json`.
- Whether the upstream corpus ships an expected pattern image beside `sample-<id>.png` is assumed.
- Live kill locks were not readable.
- `asset_thumbs.rs:392` matched by grep as a `BrushPreset`-returning helper; not opened.