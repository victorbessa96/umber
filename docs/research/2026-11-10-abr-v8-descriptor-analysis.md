# Wave 3.2 — v8+ descriptor-only `.abr` files: where the refusal is, what is missing, and the mapping decision

Planning/research output for wave 3.2 of `docs/loop/waves/2026-11-09-wave-plan.md:99-104`.
Read-only analysis of the tree at 2026-11-10. No repo file was edited, no build run.

Provenance: format knowledge is exploration results assembled from online sources (public
format write-ups and compatibility tables). No vendor tool or SDK was used. Where public
descriptions disagree, the in-repo vendored MIT reference reader is treated as the format
authority, which is already the house rule (`crates/umber-brush/src/abr.rs:22-26`).

---

## TL;DR (10 lines)

1. The refusal is one version whitelist: `walk` accepts only 6/7/9/10 (`crates/umber-brush/src/abr.rs:485-491`); a header of 8 or 11+ becomes `AbrError::UnsupportedVersion` (`abr.rs:207-213`).
2. "v8 is not a thing" (`abr.rs:482-484`) is consistent with the public record — no compatibility table lists 8 or 11 — but it is written as a format fact. Reword it as "unattested".
3. "Descriptor-only" and "v8+" are independent axes: a real v6.2 file with zero samples already imports today (`crates/umber-brush/tests/fixtures/abr_real_v6_2_full.abr`, pinned in `tests/abr_real_patt.rs:47-61`). The wave row conflates a content class with a header number.
4. The GUI path refuses v8 as well — its preflight whitelist (`crates/umber-app/src/brush_panel/abr_import.rs:141`) and the reference reader's own gate are identical. It surfaces the reference's Debug-formatted error (`unreadable .abr (StrictViolation(…))`) instead of a clean message.
5. The stamp-less mapping is already shipped: no sample → radial preset, `stamped = false`, counted in `brushes_without_sample` (`crates/umber-brush/src/abr_import.rs:160-182`, `:213-271`).
6. What a descriptor-only file lacks is only the tip bitmap; name, size, angle, roundness, spacing, hardness, dynamics and the texture link all ride in the same version-16 `desc` tree.
7. Real gap 1: a preset with no `Brsh` shape or no `Dmtr` aborts the WHOLE file (`abr.rs:609-614`, `:651`), and public analysis documents a real preset class that carries neither a sampled tip nor computed geometry.
8. Real gap 2: the CLI writes ABR diameter/2 into `lazy_mouse.radius_px` and leaves `size: 1.0` (`abr_import.rs:220-224`, `:265`), so a CLI import paints at the 24-texel default until the user resizes (`crates/umber-app/src/main.rs:1192-1194`); the GUI writes diameter/48 into `preset.size` (`brush_panel/abr_import.rs:368-370`). The two importers disagree on what ABR size means.
9. Recommendation: accept 8/11 only behind a strict `8BIM` probe, emit the radial preset (what the mapping would do anyway) plus the `Txtr` texture block, and fix per-brush geometry skip + size semantics in the same kill.
10. Two calls are genuinely the partner's: whether a tip-less brush belongs in the library at all, and whether ABR diameter may start driving painted size (a visible change to every existing import).

---

## 1. Where exactly the reader refuses them

### 1.1 The version gate

`crates/umber-brush/src/abr.rs:475-497`, verbatim:

```rust
fn walk(bytes: &[u8], mode: WalkMode) -> Result<AbrFile, AbrError> {
    if bytes.len() < 2 {
        return Err(AbrError::NotAbr);
    }

    let mut reader = Reader::new(bytes);
    let version = reader.i16()?;
    // The versions real files use (and the ones the format authority
    // accepts): 6, 7, 9, 10. v8 is not a thing; v1-v5 predate the
    // `8BIM` section layout this reader walks.
    if !matches!(version, 6 | 7 | 9 | 10) {
        return if version >= 1 {
            Err(AbrError::UnsupportedVersion { version })
        } else {
            Err(AbrError::NotAbr)
        };
    }

    let minor = reader.i16()?;
    if minor != 1 && minor != 2 {
        return Err(AbrError::UnsupportedMinorVersion { minor });
    }
    let parameter_len = if minor == 1 { 10 } else { 264 };
```

Three consequences, all before a single byte of `desc` is looked at:

* **Version 8 (and 11+) never reaches the section walk.** `src/umber-brush/src/abr.rs:485-491` — the only path from a header to `UnsupportedVersion`.
* **A v8 file also has to pass the minor gate** (`abr.rs:493-496`). A hypothetical v8 writer with a sub-version outside 1..2 fails with a *different* error (`UnsupportedMinorVersion`), so "accept v8" is not a one-line change unless the minor field is also considered.
* **The error text names the version but not the reason**: `#[error("unsupported .abr version {version}: only v6, v7, v9 and v10 are supported")]` (`abr.rs:209`). The doc comment above it asserts the descriptor-only story: "A recognized but unsupported ABR version (v1–v5, or the descriptor-only v8 and later)" (`abr.rs:207-208`).

Pinned by two tests that must be rewritten if the gate changes:

* `descriptor_only_v8_is_unsupported` (`abr.rs:1488-1502`) — builds `version=8, minor=2, 8BIM desc size 0` and asserts `UnsupportedVersion { version: 8 }`.
* `read_abr_refuses_v8` (`abr.rs:1928-1942`) — the same through the strict full read.

The module doc states the claim in prose twice: "Supported: version 6, 7, 9 and 10 (minor version 1 or 2) — the versions the reference accepts and the ones real files use" (`abr.rs:33-35`) and the non-goal "v8+ files, which are descriptor-only and rejected as unsupported" (`abr.rs:59`).

### 1.2 Is v8 really "not a thing"?

Evidence gathered here (public compatibility tables and published reader documentation):

* Every public version table found lists the modern set as **6, 7, 9, 10** with sub-versions 1/2, and the legacy set as **1, 2**. None lists 8, and none lists 11 or later.
* Two independently published readers of the format declare exactly the same version set (1, 2, 6, 7, 9, 10) and the same explicit refusal for anything else.
* One public write-up goes further and claims 7 and 9 do not exist either (its table is 1, 2, 6, 10), which shows the community tables still disagree at the edges — but it also says 8 does not exist.
* No public source found records a file whose header is 8 or 11.

Verdict: **"v8 is not a thing" is consistent with all public evidence, but it is a statement about the observed corpus, not about the format.** The in-tree comment and the `AbrError` doc assert a format fact ("the descriptor-only v8 and later") that no public source supports. The honest wording is "no attested file uses 8; 11+ is unknown" — which matters because the wave row was written on the stronger claim.

More important: **descriptor-only is not a version class.** Whether a file carries `samp` is orthogonal to its header number:

* a v6.2 file whose only brush is a `computedBrush` has an empty/absent sample list — legal, and already imported (see §3.2);
* a v10 file with only computed presets is equally samp-less;
* a v6/v10 file can mix sampled and computed presets; the CLI's synthetic v6 fixture does exactly that (`crates/umber-cli/src/import_asset.rs:683-688`: one tip shared by two sampled brushes plus one computed brush).

So the current reader does **not** refuse descriptor-only files. It refuses *unattested header numbers*, of which a descriptor-only file is one possible payload. That distinction is the core correction wave 3.2 needs.

### 1.3 The GUI path (umber-app, reference-reader based)

`crates/umber-app/src/brush_panel/abr_import.rs` does **not** use `umber_brush::abr::read_abr`; it calls the vendored reference reader (`abr_import.rs:109`). It has its own preflight walk and its own error enum:

```rust
    let (Some(version), Some(minor)) = (be16(0), be16(2)) else {
        return Ok(());
    };
    if !matches!(version, 6 | 7 | 9 | 10) || !matches!(minor, 1 | 2) {
        return Ok(());
    }
```

(`crates/umber-app/src/brush_panel/abr_import.rs:138-143`; the comment above the function says versions 1/2 and anything this walk cannot follow are passed through to the reference reader, which refuses them itself, `:130-131`.)

The reference reader's gate is the same set (`docs/claw-artifacts/*/src-tree/src/abr.rs:918-928` for 1/2/6/7/9/10, `:1048-1053` for the fallback "Unsupported ABR version ({})"; descriptor version 16 only at `src-tree/src/descriptor.rs:902-912`).

**Answer to the question as posed: no, the GUI does not accept versions that umber-brush refuses.** Both accept exactly 6/7/9/10 minor 1/2 and refuse 8/11. The differences are cosmetic and diagnostic:

| | CLI (`umber-brush`) | GUI (`umber-app`) |
|---|---|---|
| Gate | `matches!(version, 6\|7\|9\|10)` (`abr.rs:485`) | preflight whitelist (`abr_import.rs:141`) + reference gate |
| Error for v8 | `unsupported .abr version 8: only v6, v7, v9 and v10 are supported` | `unreadable .abr (StrictViolation("Unsupported ABR version (8)"))` |
| Where the text lands | wrapped as `not a readable .abr: {e}` (`import_asset.rs:564-565`) | `format!("Import failed: {err}")` (`brush_panel.rs:900-902`) |
| Translated? | no (library string) | no (raw `Display`, and the panel's labels are English until the catalog lock frees, `brush_panel.rs:876`) |

So wave 3.2 does not need a GUI fix for *acceptance*; it needs one only if the two importers are to converge on what a stamp-less brush becomes (§3.3).

---

## 2. What a descriptor-only file contains that a `samp`-carrying one does not

Structurally: **nothing extra.** It is the same `8BIM` section walk with one section missing. The section set the reader knows is `samp`, `desc`, `patt`, `phry`, everything else skipped by declared length (`crates/umber-brush/src/abr.rs:504-563`; dispatch at `:534-556`, four-byte padding at `:560-562`). A file with only `desc` (and maybe `patt`/`phry`) parses to zero samples and N brushes; the module doc states the absence is not corruption: "A file with no `samp` / `desc` section parses to empty lists rather than an error" (`abr.rs:138-140`).

What the missing `samp` changes semantically: there is no `sampledData` id to resolve, so a `sampledBrush` entry in a samp-less file is a **dangling reference**, and a `computedBrush` entry is the only shape that can describe a tip at all.

### 2.1 The descriptor fields the reader already maps

All from the brush descriptor's outer object and its `Brsh` shape object (`crates/umber-brush/src/abr.rs:609-663`):

| Key | OSType | Reader field | Line |
|---|---|---|---|
| outer class id (`_`, `MixB`, `SmTl`) | class id | `tool_class` | `:649`, `:589-596` |
| `Nm  ` (outer, else shape) | `TEXT` | `name` | `:621-624` |
| `Brsh` | `Objc` | the shape descriptor (required) | `:610-614` |
| shape class id (`computedBrush`/`sampledBrush`) | class id | `sampled` flag; other classes skipped + counted | `:615-619` |
| `sampledData` | `TEXT` | `sampled_data_id` | `:625-629` |
| `useTipDynamics` | `bool` | gate for `dynamics` | `:630-636` |
| `szVr` (`bVTy`, `fStp`, `jitter`, `Mnm `) | `Objc` | `AbrDynamics` (size dynamics only) | `:681-689` |
| `useTexture` | `bool` | gate for `texture` | `:640-646` |
| `Txtr` (`Idnt`, `Nm  `) | `Objc` | `pattern_id`, `pattern_name` | `:667-670` |
| `textureScale`, `textureDepth` | `#Prc` | `scale`, `depth` | `:671-672` |
| `InvT`, `TxtC` | `bool` | `invert`, `each_tip` | `:673`, `:677` |
| `textureBlendMode` | `TEXT`/enum | `blend_mode` (`BlnM.<code>` undecoded) | `:674-676` |
| shape `Dmtr` | `#Pxl` | `diameter_px` (**required**, see §3.4) | `:651` |
| shape `Angl` | `#Ang` | `angle_deg` | `:652` |
| shape `Rndn` | `#Prc` | `roundness` | `:653` |
| shape `Intr` | `bool` | `spacing_on` | `:654` |
| shape `Spcn` | `#Prc` | `spacing` | `:655` |
| shape `flipX`, `flipY` | `bool` | `flip_x`, `flip_y` | `:656-657` |
| shape `Hrdn` | `#Prc` | `hardness` | `:658` |

### 2.2 The descriptor fields a real brush carries that the reader parses past but does not map

Visible in the reference reader's full key walk (`docs/claw-artifacts/*/src-tree/src/abr.rs:687-888`); the module doc already names this boundary (`crates/umber-brush/src/abr.rs:63-66`):

* outer: `Wtdg` (wet edges), `Nose` (noise), `useBrushSize`, `interpretation`, `protectTexture`, `Spcn` (outer spacing, distinct from the shape's);
* shape dynamics block: `tiltScale`, `angleDynamics`, `roundnessDynamics`, `brushProjection`, `minimumDiameter`, `minimumRoundness` — `:717-731` in the reference, i.e. everything except `szVr`;
* scatter: `useScatter`, `Cnt `, `bothAxes`, `countDynamics`, `scatterDynamics`;
* texture extras: `minimumDepth`, `textureDepthDynamics`, `textureBrightness`, `textureContrast`;
* `dualBrush` (own `Brsh`, `BlnM`, `Spcn`, scatter block);
* colour dynamics: `useColorDynamics`, `clVr`, `H   `, `Strt`, `Brgh`, `purity`, `colorDynamicsPerTip`;
* transfer: `usePaintDynamics`, `prVr`, `opVr`, `wtVr`, `mxVr`;
* brush pose: `useBrushPose`, `overridePose*`, `brushPosePressure/TiltX/TiltY/Angle`;
* `toolOptions`: flow, smoothing, opacity, mode, pressure overrides, wetness/dryness/mix, `SmdF`, `SmdS`, `Prs `;
* the other shape families: `dBrush` (bristle: `Shp `, `Dnst`, `Lngt`, `clumping`, `thickness`, `stiffness`, `physics`) and `dTips` (erodible/airbrush: the `dtips*` block).

Those two families are the ones the reader deliberately counts rather than maps: "every other shape class (`dBrush`, `dTips`, ...) is skipped and counted in `AbrFile::skipped_shape_classes`" (`abr.rs:42-44`, mechanism at `:578-584`).

### 2.3 So what is actually absent from a descriptor-only file?

Only the tip pixels. Everything a stamp-less brush needs to become *some* preset is present:

* size (`Dmtr`, `#Pxl`), angle, roundness, spacing, hardness — the procedural description;
* the optional texture link (`Txtr`), which resolves against `patt` exactly as for a stamped brush (`match_pattern`, `abr.rs:412-415`; mapping `abr_import.rs:184-201`);
* the tool class and name.

And one thing is *added* in the real world: presets that declare **neither** a sampled tip nor computed geometry. A public reader's published API documents that class explicitly as a first-class outcome ("a preset that declares neither a sampled tip (`sampledData` uuid) nor computed geometry"), and a second class for "a preset skipped because its `sampledData` uuid resolved to no `samp` entry". Those are exactly the shapes a descriptor-only pack is likely to contain, and the current reader's handling of both is covered in §3.4.

---

## 3. What the current mapping already does with a missing `samp`

### 3.1 `map_file` — the fallback is the normal path

`crates/umber-brush/src/abr_import.rs:160-182`:

```rust
    for (index, brush) in file.brushes.iter().enumerate() {
        let stamp = match_sample(brush, file)
            .filter(|s| {
                s.width > 0
                    && s.height > 0
                    && s.alpha.len() as u64 == s.width as u64 * s.height as u64
            })
            .map(|s| { /* white RGB + tip alpha */ });

        if let Some(stamp) = &stamp {
            if out.entries.iter().any(|e| e.stamp.as_ref() == Some(stamp)) {
                out.duplicate_stamps_skipped += 1;
                continue;
            }
        } else {
            out.brushes_without_sample += 1;
        }
```

and `map_preset` (`abr_import.rs:213-271`) builds the radial preset for every brush, stamp or not:

```rust
    let radius = if brush.diameter_px.is_finite() {
        (brush.diameter_px / 2.0).clamp(1.0, 4096.0)
    } else { 1.0 };
    let hardness = if brush.hardness.is_finite() { brush.hardness.clamp(0.0, 1.0) } else { 1.0 };
    let dabs = if !brush.spacing_on { 100.0 } else { /* 1 / (2 * spacing), clamp 1..=100 */ };
    let degrees = if brush.angle_deg.is_finite() { brush.angle_deg as f32 } else { 0.0 };

    BrushPreset {
        name,
        params: BrushParams {
            hardness: hardness as f32,
            stamped,                       // false when no stamp
            stamp_angle: StampAngle::Fixed { degrees },
            ..BrushParams::default()
        },
        ...
        lazy_mouse: LazyMouseConfig { radius_px: radius as f32, ..Default::default() },
        spacing: SpacingConfig { dabs_per_radius: dabs as f32 },
        alpha_curve: ControlCurve::default(),
        radius_curve: ControlCurve::default(),
        stroke_method: StrokeMethod::default(),
        size: 1.0,
        catalog: String::new(),
        texture: None,                     // set later by the CLI write path
    }
```

Notes on that literal, because they matter for the decision:

* `stamped: false` makes the engine paint the **radial falloff** profile (`crates/umber-brush/src/dab_adapter.rs:213-218`), i.e. a round dab whose edge is `params.hardness`. The descriptor's `Hrdn` is already carried into exactly that field (`crates/umber-brush/src/abr_import.rs:225-229`), so option (b)'s hardness mapping is the hardness core, not a curve.
* `texture: None` is deliberate: the preset's `texture` block is filled by the CLI plan step from `AbrImportEntry::texture` (`crates/umber-cli/src/import_asset.rs:583-611`). It is not stamp-dependent, so a radial preset can carry a texture.
* The `duplicate_stamps_skipped` rule only compares existing stamps (`:175-179`), so a pack of stamp-less brushes is **not** collapsed: every preset imports.

### 3.2 A real descriptor-only file already imports (shipped, pinned)

`crates/umber-brush/tests/fixtures/abr_real_v6_2_full.abr` is the untrimmed upstream `simple/src.abr` — a real v6.2 file with **zero samples**, one `computedBrush` named "Soft Round" (upstream expected values in `tests/fixtures/abr_real_v6_2_full.data.json`: `size 30, angle 0, roundness 1, spacingOn true, spacing 0.25, hardness 0`). `tests/abr_real_patt.rs:47-61` pins:

```rust
    assert_eq!(imported.entries.len(), 1);
    ...
    assert_eq!(entry.preset.name, "Soft Round");
    assert!(entry.stamp.is_none(), "Soft Round is a computed brush");
    assert_eq!(imported.brushes_without_sample, 1);
```

and `tests/abr_real_desc.rs:24-30` pins the reader side on the same fixture (`read_abr` → 0 samples, 1 brush, "Soft Round" matching upstream `data.json`); the trimmed sibling `abr_real_v6_2_no_samples.abr` is pinned samples-only at `tests/abr_real_file.rs:104-109`. The wave-abr close report already recorded the mapped result: `name "Soft Round", stamped=false, hardness 0.0, radius_px 15.0 (30/2)` (`docs/loop/kill-reports/2026-11-08-wave-abr-complete.md:25`).

**So the "what does a stamp-less brush become" question already has a shipped answer: a radial preset, counted.** Wave 3.2 cannot be "decide this from scratch"; it can only revisit the answer (and the size field it uses).

### 3.3 The GUI disagrees on the same input

`crates/umber-app/src/brush_panel/abr_import.rs:330-367`:

```rust
            BrushShape::Sampled { size, angle, spacing_on, spacing, sampled_data, .. } => {
                let Some(sample) = samples.iter().find(|s| &s.id == sampled_data) else {
                    out.report.push(format!(
                        "{name}: its tip image is missing from the file; skipped"
                    ));
                    continue;
                };
                ...
            }
            BrushShape::Tips { .. } | BrushShape::Dynamic { .. } => {
                out.report.push(format!(
                    "{name}: an erodible/airbrush/bristle tip, which Umber cannot paint yet; skipped"
                ));
                continue;
            }
```

* GUI: a `sampledBrush` with a dangling id is **skipped** with a report line; `dBrush`/`dTips` are skipped.
* CLI: a dangling `sampledData` id becomes a **radial preset** (`abr_import.rs:180-182`; pinned by `unmatched_sampled_id_is_radial_and_counted`, `:426-436`); `dBrush`/`dTips` are skipped by the reader and counted in `skipped_shape_classes` (on `AbrFile`; `map_file` then drops the count).
* Size: GUI writes `preset.size = (size / BASE_DIAMETER_PX).clamp(0.05, 20.0)` with `BASE_DIAMETER_PX = 48.0` (`brush_panel/abr_import.rs:38-41`, `:368-370`) and leaves `lazy_mouse` at its default; CLI writes `size: 1.0` and puts `diameter/2` in `lazy_mouse.radius_px`.
* Spacing: same formula, different clamps — CLI `1..=100` dabs/radius (`abr_import.rs:230-239`), GUI `0.05..=50` (`brush_panel/abr_import.rs:374-378`).
* Angle: GUI only sets `StampAngle` when `angle != 0.0` (`:379-383`); CLI always sets `Fixed { degrees }` (`:240-244`).

The size divergence is not cosmetic. The painted radius comes from `preset.size` (`crates/umber-app/src/main.rs:1192-1194` → `paint.set_brush_size(preset.size)`), and the base radius is 24 texels (`crates/umber-paint/src/paint_state.rs:586`, `:4528`). `lazy_mouse.radius_px` is the pulled-string leash, documented as "Leash length in logical pixels" (`crates/umber-brush/src/preset.rs:191-205`). A CLI import therefore paints every brush at the session default size until the user resizes it, while the GUI import paints at diameter/48 of the 24-texel base. Neither importer is self-evidently right; both are visible.

### 3.4 So what is precisely missing for v8+?

Ordered by how much they block a legal stamp-less file:

1. **Version acceptance** — the gate in §1.1. Nothing else in the walk cares about the header number, but see (2).
2. **Minor-version acceptance** — `abr.rs:493-496` rejects any sub-version outside 1/2. A v8/11 writer's sub-version is unknown; if it is 0 or 3 the file still fails, with the less helpful `UnsupportedMinorVersion`.
3. **Per-brush strictness.** `parse_brush` returns an error — aborting the whole file — when the descriptor has no `Brsh` shape descriptor (`abr.rs:610-614`, "a brush has no tip-shape descriptor") or when `Dmtr` is missing / not in `#Pxl` (`abr.rs:651` → `parse_units_to_number`, `abr_desc.rs:315-320`, "missing a value in Pixels units"). A descriptor-only pack is exactly where geometry-less presets concentrate (§2.3), so one bad preset currently costs the entire import. The reference reader is equally strict here (`docs/claw-artifacts/*/src-tree/src/abr.rs:688-689`, `:501`), so "the reference refuses it too" is true — but it is still the wrong behaviour for an importer whose contract is "report per brush, never fail the pack".
4. **Descriptor version 16 only** (`abr_desc.rs:86-87`, `:226-233`; reference `src-tree/src/descriptor.rs:902-912`). Every public description says the wrapper is 16; nothing suggests a newer value, so this is a low-probability block, but it is a hard one.
5. **Unknown OSType is a hard error** (`abr_desc.rs:25-30`: the descriptor has no per-item length, so the value cannot be skipped). One unfamiliar value type anywhere in the tree kills the file. For an "unknown future version" this is the sharpest edge after the version gate.
6. **Section-level layout is fine.** The walk skips unknown `8BIM` keys by declared length (`abr.rs:504-563`), so a new section kind is harmless; `phry` (hierarchy) is already parsed and ignored (`:543-546`); `patt` is decoded (`:547-556`). No 8BIM-id changes are needed for a descriptor-only file.
7. **Mapping-level**: nothing. As shown in §3.1-3.2, a `computedBrush` with no sample already maps to a radial preset with size/spacing/hardness/angle and an optional texture.

One behavioural trap worth naming: if a descriptor-only pack consists only of `dBrush`/`dTips` presets, the reader returns **zero brushes** with `skipped_shape_classes > 0`, the CLI plans an empty import, writes nothing, and returns `Ok` with `presets: 0` (`import_asset.rs:643-660`). That is "silently imported nothing", not an error.

---

## 4. Mapping options for a stamp-less descriptor-only brush

Common to all three: the descriptor's `Dmtr`, `Angl`, `Spcn`/`Intr`, `Hrdn` are available; `Txtr` resolves against `patt` when the pattern record is present; dynamics are parsed only for `szVr` and dropped by the mapping (`abr_import.rs:49-53` names this a v1 non-goal).

### (a) Keep refusing, with an error that names v8+ and says why

Change `UnsupportedVersion`'s message (and the doc comment at `abr.rs:207-208`) to something like "unsupported .abr version {version}: supported versions are 6, 7, 9 and 10; 8 and 11+ are not attested and are refused".

* **Buys:** zero risk of mis-parsing an unknown layout; no behaviour change; one small diff.
* **Costs:** the wave row stays open forever with nothing user-visible gained, because no public source shows such a file exists. It also keeps a false statement in the docs ("v8+ files, which are descriptor-only") — the refusal is about the header, not the content.
* **Risks:** if a partner or a user does have a v8-labelled pack, they get a clean refusal and no workaround; and the message implies we know what v8 is, which we do not.

### (b) Accept and emit the radial fallback preset

Accept 8/11 (behind a probe, see §5/E1) and let the existing `map_file`/`map_preset` path do what it already does: `stamped = false`, radius from `diameter_px / 2`, hardness from `Hrdn`, `dabs_per_radius` from `Spcn`, `stamp_angle` from `Angl`, counted in `brushes_without_sample`.

* **Buys:** a tip-less brush becomes something paintable and named; no new mapping code (the path is already pinned); a v8 file that is really a v6-style descriptor-only file imports correctly.
* **Risks:**
  * *Wrong look, honestly labelled*: a "Chalk" or "Splatter" descriptor-only brush becomes a smooth round dab. The name promises a tip the file does not contain.
  * *Wrong size semantics* (pre-existing, §3.3): with `size: 1.0` the brush paints at 24 texels regardless of `Dmtr`. Fixing that is a change to every existing CLI import, not just v8 files.
  * *Dynamics unmapped*: the descriptor's jitter/fade/angle-dynamics do not reach the engine (wave 3.3's row), so a dynamic brush becomes static.
  * *Acceptance risk*: unless the acceptance is probed (§5/E1), a mislabelled or corrupt file with header 8 can reach the descriptor parser and produce a partial import or a confusing descriptor error.
  * *GUI/CLI divergence*: the GUI still refuses the file and still skips dangling sampled presets, so the same pack imports in the CLI and not in the GUI.

### (c) Accept, emit a params-only preset, attach the texture block when `Txtr` resolves

Same as (b), plus the already-shipped texture plumbing: `AbrImportEntry::texture` → `preset.texture` → `<slug>.pattern.png` sidecar (`import_asset.rs:583-611`, `:634-638`; `preset.rs:345-354`).

* **Buys:** the descriptor's `useTexture`/`Txtr`/scale/depth/invert/blend is not thrown away; a v8 pack with patterns keeps its grain parameters.
* **Risks:** the engine does not sample the texture yet (kill 3.1c is the consumer, `preset.rs:349-352`), so the user sees a round dab *and* a texture block that paints nothing different — an expectation gap unless the report says so. The pattern sidecar adds files to the library and an extra failure mode (`TargetExists` refuses the whole import, `import_asset.rs:639-641`).

**Recommendation: (b) + (c), with the acceptance probed rather than blanket.** Concretely: accept version 8 and 11 only when the body walks as `8BIM` sections and the `desc` yields at least one `Brsh` entry under descriptor version 16; otherwise keep a named refusal (a new `UnsupportedLayout`-style variant, or `UnsupportedVersion` with a better message). That is (b)'s user value without (b)'s main risk, and (c) is nearly free because the texture path is already unconditional on `stamped`.

Do (b)+(c) **only together with** E3 (per-brush geometry skip) and at least a decision on E5 (size semantics), otherwise the new acceptance delivers a library full of identically-sized round dabs — technically correct, practically useless.

---

## 5. Decisions: partner vs engineering

### Partner / product questions

* **Q1. Does a tip-less brush belong in the library at all?** Today: CLI says yes (radial preset), GUI says no for a missing tip ("its tip image is missing from the file; skipped", `brush_panel/abr_import.rs:338-343`) and yes for a computed brush. This is a user-visible product call, and it decides whether wave 3.2 is "import more" or "refuse better".
* **Q2. May ABR diameter start driving painted size?** Today the CLI writes it to the lazy-mouse leash and `size: 1.0` (§3.3). Making `Dmtr` set `preset.size` changes the look of every brush imported so far — a partner-visible behaviour change, and the obvious prerequisite for a tip-less brush being the size the artist expects.
* **Q3. Should a preset with neither a tip nor computed geometry be imported as a round brush, or skipped and counted?** Public analysis documents the class; the current reader refuses the whole file on it (§3.4 item 3). "Round brush" is a claim about a brush we have no geometry for.
* **Q4. Unify the two importers now?** The GUI and the CLI still disagree on: version error text, dangling-tip handling, size mapping, spacing clamps, angle zero, and textures (the GUI drops patterns). 3.1's Q2 is still open (`docs/research/2026-11-09-abr-patt-analysis.md:278`), and 3.2 re-opens it on the same file.
* **Q5. Risk appetite for unattested headers.** If a real 8/11 file exists in the wild, is a best-effort import (with a warning) better than a clean refusal — given we cannot promise the descriptor semantics are unchanged?
* **Q6. Empty import is success?** A descriptor-only pack of `dBrush`/`dTips` presets imports nothing and exits `Ok` with `presets: 0` (§3.4). Should the CLI/GUI surface that as a warning the user cannot miss?

### Engineering decisions the dragon can take

* **E1. Gate shape.** Whitelist `6|7|9|10` (status quo) vs `6|7|8|9|10|11` + structural probe. If accepting: parse the header, then require the first section signature to be `8BIM`, require the minor to be 1/2 (or probe the parameter length), and require ≥1 parsed `desc` brush; on any of those failing, return the version error. Also decide whether 12+ is refused (yes, until attested).
* **E2. Minor gate for the unattested versions.** Keep `1|2` and document that a v8 file with another sub-version is still refused with `UnsupportedMinorVersion` (cheapest, honest), or probe it.
* **E3. Per-brush geometry skip.** Turn the two whole-file aborts (`abr.rs:610-614`, `:651`) into "skip this brush, count it, import the rest", with a new counter (e.g. `presets_without_geometry`) plumbed to the CLI outcome (additive JSON, no compat break) and to the GUI report. This is the single change that most improves descriptor-only imports, and it is independent of the version question.
* **E4. Descriptor strictness.** Keep descriptor version 16 hard, keep unknown OSType hard (no per-item length), but make a *per-entry* descriptor failure skip that entry where the list length allows recovery (the `Brsh` list is length-delimited by item count, so a failed entry cannot be resynchronised — probably keep it hard and say so). Name the choice in the module doc.
* **E5. Size mapping.** Pick one definition of ABR `Dmtr` (painted diameter vs diameter/48 of a 24-texel base) for both importers, and decide whether the leash mapping stays. Clamp ranges for GUI parity (`1..=4096` vs `0.05..=20.0` size; `1..=100` vs `0.05..=50` dabs/radius).
* **E6. Error copy and its home.** `AbrError` messages are untranslated library strings surfaced raw by both surfaces (`import_asset.rs:564-565`, `brush_panel.rs:900-902`). A "clearer error naming v8+" is a wording change in `umber-brush`; a translated one needs a key in both `.ftl` files and an umber-app mapping — an extra file surface for wave 3.2.
* **E7. Fixtures.** No real v8/11 file exists to test. The honest fixture set is: a synthetic `version=8, minor=2, 8BIM desc` file (the shape of the current pin, `abr.rs:1488-1502`) and a real descriptor-only file (already vendored, §3.2). Do not claim real-file coverage.
* **E8. Counts and reports.** `skipped_shape_classes` already exists but never reaches the CLI outcome or the GUI report. If 3.2 touches the mapping, surfacing it is the cheap way to make "imported nothing" visible (Q6).

### Proposed kill split (if (b)+(c) is chosen)

* **3.2a** `abr.rs`: version/minor gate + structural probe + per-brush geometry skip + E4 decision, pins in `abr.rs`; fixtures synthetic + the existing real descriptor-only file. No mapping change.
* **3.2b** `abr_import.rs` + `umber-cli/src/import_asset.rs`: confirm the radial fallback for descriptor-only packs (already true), add `presets_without_geometry`/`skipped_shape_classes` to the outcome, decide texture-on-radial, pins on a synthetic v8 pack and on the real v6.2 file. Depends on Q1/Q3.
* **3.2c** size semantics (E5) — separate, because it changes every existing import and needs Q2.

---

## 6. Pins a kill should write (if (b)+(c) is chosen)

1. A synthetic `version=8, minor=2` file with one `desc` `computedBrush` (`Dmtr 30 #Pxl`, `Spcn 25 #Prc`, `Hrdn 0 #Prc`, `Intr true`) returns `Ok`, `brushes.len() == 1`, and imports to one radial preset with the exact mapped values.
2. The same file with `version=12` still returns `UnsupportedVersion { version: 12 }`.
3. A `version=8` file whose body is not `8BIM` returns the named refusal, not a panic and not a partial import.
4. A `version=8, minor=3` file returns `UnsupportedMinorVersion { minor: 3 }` (documented boundary).
5. A `desc` with one geometry-less brush plus one good `computedBrush`: the good one imports, the bad one is counted, `Ok` (E3).
6. Byte-compat: every existing v6/7/9/10 fixture imports to exactly the entries/counts it does today (the real-file tests in `tests/abr_real_*.rs` must pass unchanged).
7. A descriptor-only brush with `useTexture` + `Txtr` + a matching `patt` record gets a `texture` block and a `.pattern.png` sidecar while staying `stamped: false`.
8. The current v8 pins (`abr.rs:1488-1502`, `:1928-1942`) are either deleted or rewritten to the new contract — a leftover `#[test]` asserting the old refusal would make the intent ambiguous.

---

## 7. Unverified / gaps

* **No real v8 or v11 file exists to test against.** Everything about their internal layout is inference from the version 6/10 files; the "probe" in E1 is the mitigation, not a proof.
* **Whether the format's vendor ever writes 8 or 11 is unknown publicly.** The version tables agree on the absence; none explains why 8 is skipped.
* **Whether a real descriptor-only pack's presets all carry `Brsh`/`Dmtr`.** Public API documentation says a geometry-less preset class exists; no sample file is in the tree to confirm what the reader would do with it end to end (only the code path, §3.4).
* **GUI behaviour not exercised.** The GUI mapping and its skip paths are read, not run; the size/leash consequence is reasoned from `main.rs:1192-1194` and `paint_state.rs:586`, not measured on screen.
* **Section layout of a hypothetical v8 body.** Assumed identical to v6/v10 (`8BIM` + key + u32 size + padding). No source describes a different layout for a header we have never seen.
* **Public tables disagree at the edges** (one write-up denies 7 and 9). The in-repo reader accepts them; that choice is the reference's, not a documented format fact.

## 8. Sources

* In-repo: `crates/umber-brush/src/abr.rs`, `abr_desc.rs`, `abr_import.rs`; `crates/umber-app/src/brush_panel/abr_import.rs`, `brush_panel.rs`, `main.rs`; `crates/umber-cli/src/import_asset.rs`; `crates/umber-paint/src/paint_state.rs`; `crates/umber-brush/tests/fixtures/FIXTURES.md` and the three real fixtures.
* In-repo vendored MIT reference reader: `docs/claw-artifacts/*/src-tree/src/abr.rs` (version gate `:918-928`, `:1048-1053`; shape parse `:498-571`; full key walk `:687-888`), `src-tree/src/descriptor.rs` (`:902-912`).
* Online: two community ABR compatibility tables (both list 1, 2, 6, 7, 9, 10; neither lists 8 or 11), one of which states explicitly that versions 3-5 and 7-9 "do not exist"; an independently published Rust reader's crate documentation (same version list, plus the tip-less/geometry-less preset classes used in §2.3); a vendor community-forum thread (snippet-level) describing the `samp`/`patt`/`desc` section order. Exploration results assembled from online sources; where they disagree with the in-repo reference reader, the reference wins (house rule, `abr.rs:22-26`).
