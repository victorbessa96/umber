# Dogfood + First-Run Walkthrough Prep Brief (Gates 8/C2-24, C2-30)

**Date**: 2026-11-17
**Context**: v0.1.0-alpha.1 released 2026-11-16 17:34Z (tag `v0.1.0-alpha.1` @ `bc4d642f`). SPEC v1.1 ratified (gate 7 MET). **Gate 9 is MET**; Gate 8 (dogfood C2-24) remains Bessa's call. This brief prepares the dogfood pass and the first-run walkthrough script.

**2026-11-24 status correction:** Git ancestry proves D2 `533560e8`, D3 `c499255f`, D4 `35097df5`, D5 `752bc991`, and displacement `9e0517bf`/`0654ba48` are **post-alpha**. The published alpha cannot exercise those steps. Use it only for behavior present at `bc4d642f`; use a newer explicit build for the D2–D5/displacement sections below. This document's C2-30 label is the first-run walkthrough, not the closed Windows Ink C2-30 trace.

**Lock**: `razul-dogfood-prep` on `docs/loop/briefs/2026-11-17-dogfood-walkthrough-prep.md`

---

## 1. Release Artifact Baseline (v0.1.0-alpha.1)

| Asset | Size | SHA-256 |
|-------|------|---------|
| `umber-app-x86_64-pc-windows-msvc.zip` | 18,629,260 B (18.6 MB) | `5014b59583b05cdcbf4b74e70f59835c3a970f011208f185efe24cc5c8610008` |
| `umber-app-x86_64-unknown-linux-gnu.tar.xz` | 14,005,884 B (14.0 MB) | `5ca958389e44f1511c0362d1365ceaaf5a4ec9bf0c2c61232c0ac3d8d398a8d8` |
| `umber-cli-x86_64-pc-windows-msvc.zip` | 8,412,671 B (8.4 MB) | `e9a69009be98957aa2448deba73f40057befc5b2bcebb3f41fb39b82995359db` |
| `umber-cli-x86_64-unknown-linux-gnu.tar.xz` | 5,523,280 B (5.5 MB) | `fd0b7f95b44512a180be0073a987b947ee1462d5a263d1e49874c63bb7e258b3` |
| `source.tar.gz` | 9,236,875 B (9.2 MB) | `1d601a4c9d4a3ca5b3d9767876a3d19e51b5943183b2dccb2e5af7d96c4da664` |
| `umber-app-installer.sh` | 51,176 B | `8ffe72aa1f9f613a1bfabfc96b5f1993bb595a9db454c15956c8fffc24a787a7` |
| `umber-app-installer.ps1` | 22,352 B | `2f31560e9fcd1e7577aa0ee125b1613110748b039339457054b93c6d9404b4e6` |
| `umber-cli-installer.sh` | 51,192 B | `19a6894989fe38fd2ee3519108875009809749422e474acb41603d8925697dd2` |
| `umber-cli-installer.ps1` | 22,364 B | `9d656338327d4a1436ab3d7c470bb42ac9abf2aa3510b8f52a1982ef83ebfd29` |
| `sha256.sum` | 503 B | `af8c23867468a603d1b98d5d73c7d81ed3a15041b159e5baca95592d4fe1e156` |
| `dist-manifest.json` | 76,814 B | `790000f40bb4d73a51b6d205b0ce10e09429b47b92fbce5e7604420e7e3d13f5` |

**Budget check**: All app/CLI archives **< 150 MB** (max 18.6 MB). ✅

**Signing**: Unsigned per C2-22 (Bessa 2026-11-16). Windows shows SmartScreen "More info → Run anyway". Checksums verified via `sha256sum -c` on Linux; `Get-FileHash` on Windows.

---

## 2. Post-Alpha Features to Exercise on a Newer Build (D2–D5 + Displacement Preview)

| Feature | Status | Key UI / Controls | Kill Report / Brief |
|---------|--------|-------------------|---------------------|
| **D2 Auto-cage** | ✅ Landed | Bakes panel → High-poly group → "Auto cage" checkbox + "Cage method" combo (Inflate / Shrinkwrap / Manual). Viewport cage wire overlay toggled via panel state. | `docs/loop/kill-reports/2026-11-16-d2-auto-cage.md` |
| **D3 Opacity baker** | ✅ Landed | Bakes panel → opt-in "Opacity" map. Source selector (Cavity / ID map / Manual), strength, and invert; RGBA output with opacity in R and coverage in A. | `docs/loop/kill-reports/2026-11-16-d3-opacity-baker.md` |
| **D4 ID sources (multi-source ID)** | ✅ Landed | Bakes panel → "ID Multi" map. Source list: Triangle / Material (gated) / IdMap / Manual. Per-source weight, optional mask path, color. Blend mode: Additive / Max / Replace. | `docs/loop/kill-reports/2026-11-16-d4-id-sources.md` |
| **D5 Ground plane** | ✅ Landed | Bakes panel → "Ground Plane" map. Settings: size, height offset, subdivisions, maps (AO/Height/ID). Viewport wire preview toggle. | `docs/loop/kill-reports/2026-11-16-d5-ground-plane.md` |
| **Displacement preview** | ✅ Landed | Overlays popover → displacement toggle + strength slider. Mesh vertices displace along normals from the baked height texture; default off and CPU reference parity pinned. | `docs/loop/kill-reports/2026-11-17-displacement-preview.md` |

**Already-present bakes panel features** (baseline for dogfood):
- Async bake (non-blocking, one job at a time)
- UDIM tile selector (per-tile AO; other maps tile 1001 only in v1)
- Skew correction (`bake_skew` checkbox + threshold slider; log warns clamped texels)
- Auto-rebake (500 ms debounce on param change; fires affected maps × sets × tiles only)
- Bake log (timestamp, set, map, tile, duration, skew-clamped warnings; click → jump to responsible setting)
- Margin types: Extend (default) / Adjacent Faces (copies partner island across seam)
- Linked bake groups (R-D1): link sets → shared settings block; copy/paste settings; sync carry persists in `.umber` project
- High-poly transfer: AO, Normal, Curvature, Thickness, Position, WorldNormal, BentNormal, Skew
- Multi-set: enable/disable per texture set; "Bake all sets" re-enables everything

---

## 3. Dogfood Session Plan (Gate 8 / C2-24)

**Goal**: On a **newer explicit build**, paint one two-material prop end-to-end using the **post-alpha D2–D5 features**, record observations, and verify export round-trip. The published alpha is valid only for its tag-contained feature set.

**Test asset**: Bessa's two-material prop (the C2-24 reference). If unavailable, use a two-material FBX with UVs (e.g., generated `cube_uv_ascii7400.fbx` or a simple prop with two texture sets).

**Environment**: Fresh Umber profile (no prior config). Linux preferred (primary dev platform); Windows as secondary if time permits.

### 3.1 Session Steps

| Step | Action | Expected Result | Record |
|------|--------|-----------------|--------|
| **1. Fresh launch** | `./umber-app` (or installer) with clean config dir (`~/.config/umber` removed) | App opens to empty project, no crash. First-run state: default panels, no mesh loaded. | Screenshot of empty workspace; note any first-run dialogs. |
| **2. Import mesh** | File → Import → select two-material FBX | Mesh loads; two texture sets appear in Bakes panel (e.g., "Body", "Trim"). UVs valid. | Screenshot of texture sets list; note set names. |
| **3. Verify viewport** | Orbit, pan, zoom; toggle wireframe/grid | Viewport responsive; mesh renders with default PBR (no paint yet). | Note FPS feel; any visual artifacts. |
| **4. Paint base layers** | Create fill layer per set; paint color/roughness/metalness on each | Brush works; layers stack; undo/redo functional. | Record layer count; any lag on 4K strokes. |
| **5. Bake AO (baseline)** | Bakes panel → enable AO → Bake | Async bake starts; button disables; completes; log entry appears; AO maps written to output dir (`<set>_ambient_occlusion.png`). | Log entry text; file sizes; open AO in external viewer to verify coverage. |
| **6. D2 Auto-cage** | Enable "Auto cage" → set method to **Inflate** → set Front/Back offsets → Bake AO again | Cage generated (CPU); GPU cage buffer uploaded; AO bake uses cage (ray origin offset); log shows same map baked again. Viewport cage wire overlay appears when panel has `last_cage`. | Screenshot of viewport with cage overlay; log entry; compare AO with/without cage (visual diff). |
| **7. D2 Cage method compare** | Switch to **Shrinkwrap** → Bake AO → switch to **Manual** → Bake AO | Three distinct cage behaviors: Inflate (raycast outward), Shrinkwrap (closest point on high), Manual (implicit `front*normal`). | Record which method feels correct for the prop; note any artifacts. |
| **8. D4 ID Multi** | Enable "ID Multi" → add sources: **Triangle** (weight 1.0) + **Manual** (weight 0.5, color #FF0000) → Blend: **Additive** → Bake | Composite RGBA8 written: Triangle hash + red tint additively blended. No mask needed. | Open ID Multi output; verify distinct per-triangle colors + red bias. |
| **9. D4 ID Multi — IdMap source** | Add **IdMap** source (point to existing baked ID PNG) → weight 1.0 → Blend: **Max** → Bake | IdMap sampled as texture; Max blend picks brightest per channel. | Verify IdMap contribution visible; test mask gating if mask path provided. |
| **10. D4 ID Multi — Replace blend** | Switch blend to **Replace** → Bake | First source with coverage wins per texel. | Confirm Replace behavior (Triangle wins where it covers). |
| **11. D5 Ground plane** | Enable "Ground Plane" → Size: 10, Height: -0.1, Subdivs: 4, Maps: AO+Height+ID → Bake | Three maps written: `<set>_ground_plane_ao.png`, `_height.png`, `_id.png`. Viewport wire preview shows plane grid at height offset. | Screenshot of viewport wire preview; open Height map — should be flat at height offset; AO shows contact shadows; ID shows plane triangles. |
| **12. D5 Ground plane — viewport preview toggle** | Toggle ground plane wire overlay off/on in viewport | Overlay appears/disappears without rebake (byte-identical-off). | Confirm instant toggle; no flicker. |
| **13. Skew correction** | Enable "Skew" → set threshold → Bake AO | Skew map written (`<set>_skew.png`); log warns clamped texels if any. V1 skew files are flat (zero measured deviation — GPU emission wiring is follow-up). | Log entry shows clamped count; skew map is flat (expected). |
| **14. Auto-rebake** | Enable "Auto rebake" → change AO rays slider → wait 500 ms | Only AO rebakes (not curvature/position/etc.); log shows AO entry only; button stays disabled during job. | Verify debounce fires; only affected map baked. |
| **15. Bake log jump-to-setting** | Click log entry for AO (clamped) → click log entry for Skew | Highlight moves to "Rays" row for AO clamped; to "Skew threshold" for Skew entry. Row label shows "▶" prefix. | Verify jump works for each map type. |
| **16. Linked bake groups (R-D1)** | Link the two texture sets → change rays on Set A → verify Set B reflects change → unlink Set B → change rays on Set A → verify Set B unchanged | Settings sync across linked sets; unlink materializes copy; further edits diverge. | Confirm group behavior; test copy/paste settings across sets. |
| **17. Margin types** | Set margin to **Adjacent Faces** → Bake curvature (has seams) → compare with **Extend** | Adjacent Faces copies partner island across seam; Extend repeats own border. | Visual diff at seam; note which looks correct for prop. |
| **18. UDIM (if multi-tile mesh)** | Load multi-tile FBX → tile selector shows 1001, 1002… → select 1001+1002 → Bake AO | Per-tile AO written: `<set>_ambient_occlusion_1001.png`, `_1002.png`. Curvature/Position skip non-1001 tiles (reported skipped). | Verify tile naming; skipped maps logged correctly. |
| **19. Export (ORM/glTF)** | File → Export → glTF preset → select all painted channels + baked maps → Export | Exported `.gltf` + textures; ORM packing correct (Occlusion=R, Roughness=G, Metalness=B). Baked maps included per preset. | List exported files + sizes; open in glTF viewer (e.g., glTF Viewer) to verify. |
| **20. Save / reopen project** | Save `.umber` → quit → relaunch → open project | All layers, bake settings, linked groups, sync carry restored. Viewport state (cage/ground plane toggles) restored. | Verify persistence; note any lost state. |
| **21. Displacement preview** | Enable a height-map bake → enable the displacement-preview overlay → adjust the strength slider | Vertex displacement is visible; strength scales it linearly; disabled or missing-height cases are no-ops. | Screenshot at two strengths; note any shader artifact. |

### 3.2 Recording Template (per session)

```
DOGFOOD SESSION LOG — 2026-11-XX
Platform: Linux / Windows
Umber version: <newer explicit build> (<commit SHA>; required for post-alpha rows)
Mesh: <name> (<n> texture sets, <n> tiles)
Output dir: <path>

STEP | FEATURE | PASS/FAIL | NOTES
-----|---------|-----------|------
1    | Fresh launch |  |
2    | Import mesh |  |
3    | Viewport baseline |  |
4    | Paint base layers |  |
5    | Bake AO (baseline) |  |
6    | D2 Auto-cage (Inflate) |  |
7    | D2 Cage methods |  |
8    | D4 ID Multi (Triangle+Manual) |  |
9    | D4 ID Multi (IdMap) |  |
10   | D4 ID Multi (Replace) |  |
11   | D5 Ground plane |  |
12   | D5 Viewport preview |  |
13   | Skew correction |  |
14   | Auto-rebake |  |
15   | Bake log jump |  |
16   | Linked groups |  |
17   | Margin types |  |
18   | UDIM |  |
19   | Export (glTF/ORM) |  |
20   | Save/reopen |  |
21   | Displacement preview |  |

OPEN PRODUCT QUESTIONS ANSWERED:
- Q1: ...
- Q2: ...

BLOCKERS / REGRESSIONS:
- ...

TIME SPENT: ~XX minutes
```

---

## 4. First-Run Walkthrough Script (C2-30)

**Goal**: A new user downloads the installer, runs it, opens a mesh, bakes each **new** map (D2–D5), exports, and verifies file sizes. This is the "happy path" for the release announcement / README.

**Build coverage rule (2026-11-24):** the alpha URLs below cover only the `bc4d642f` feature set. Steps 5b–5e and every D2–D5/displacement row require a newer explicit build and its authorized artifact URL; do not record those steps against `v0.1.0-alpha.1`. For an alpha-only walkthrough, stop at tag-contained functionality.

**Audience**: First-time Umber user (technical artist / indie dev). Assumes zero prior Umber knowledge.

**Platform**: Linux (primary) + Windows (secondary). Use the **installer scripts** (`umber-app-installer.sh` / `.ps1`) — they verify checksums, unpack to `~/.local/share/umber` / `%LOCALAPPDATA%\Umber`, add to PATH, create desktop entry.

### 4.1 Script (Linux)

```bash
# 0. Clean slate (simulate fresh machine)
rm -rf ~/.config/umber ~/.local/share/umber
rm -f ~/Desktop/umber-app.desktop  # if exists

# 1. Download + verify (user-facing)
wget https://github.com/victorbessa96/umber/releases/download/v0.1.0-alpha.1/umber-app-installer.sh
wget https://github.com/victorbessa96/umber/releases/download/v0.1.0-alpha.1/sha256.sum
sha256sum -c sha256.sum 2>&1 | grep umber-app-installer.sh
# Expected: umber-app-installer.sh: OK

# 2. Run installer (non-interactive, default prefix)
chmod +x umber-app-installer.sh
./umber-app-installer.sh --prefix ~/.local --yes
# Installs to ~/.local/share/umber, adds ~/.local/bin to PATH, creates desktop entry

# 3. Launch (first run)
umber-app
# → App opens. Note: no "welcome wizard" yet; empty project is the first screen.

# 4. Open test mesh (drag-and-drop or File → Import)
# Use a simple two-material FBX with UVs (e.g., the generated cube_uv_ascii7400.fbx or a provided sample)
# → Mesh loads. Two texture sets appear in Bakes panel (left dock).

# 5. Bake each NEW map (D2–D5) — one at a time, verify output
# 5a. AO (baseline, pre-D2)
#     Bakes panel → enable "Ambient Occlusion" → Bake
#     → Wait for async job. Log entry appears.
#     → ls -la output_dir/*ambient_occlusion*.png
#     Expected: ~500 KB–2 MB per 2K/4K map.

# 5b. D2 Auto-cage (Inflate)
#     High-poly group → enable "Auto cage" → Method: Inflate → Front: 0.001 / Back: 0.01 → Bake
#     → Viewport: cage wire overlay appears (thin lines around mesh).
#     → Output: same AO map overwritten (cage-applied).

# 5c. D4 ID Multi
#     Enable "ID Multi" → Sources: Triangle (weight 1.0) + Manual (weight 0.5, color #00FF00) → Blend: Additive → Bake
#     → Output: <set>_id_multi.png (RGBA8, distinct per-triangle + green tint).

# 5d. D5 Ground Plane
#     Enable "Ground Plane" → Size: 5, Height: -0.05, Subdivs: 2, Maps: AO+Height+ID → Bake
#     → Output: three files: <set>_ground_plane_ao.png, _height.png, _id.png
#     → Viewport: toggle "Ground plane" wire preview (Overlays popover) → grid appears at height.

# 5e. (If landed) Displacement Preview
#     Bake Height (transfer) → enable Displacement preview toggle → Strength: 0.1
#     → Viewport: mesh vertices displaced along normal by height * strength.

# 6. Export
#     File → Export → glTF (Unreal preset) → check all channels → Export to ~/umber-export/
#     → Verify files:
ls -la ~/umber-export/
# Expected: .gltf + .bin + textures (baseColor, normal, ORM, emissive, + baked maps per preset)
# File sizes: each texture ~500 KB–4 MB; total export < 50 MB for a simple prop.

# 7. Save project
#     File → Save As → ~/my-prop.umber
#     Quit → relaunch umber-app → File → Open → ~/my-prop.umber
#     → All layers, bake settings, linked groups restored.
```

### 4.2 Script (Windows PowerShell)

```powershell
# 0. Clean slate
Remove-Item -Recurse -Force $env:APPDATA\umber -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $env:LOCALAPPDATA\Umber -ErrorAction SilentlyContinue

# 1. Download + verify
Invoke-WebRequest -Uri "https://github.com/victorbessa96/umber/releases/download/v0.1.0-alpha.1/umber-app-installer.ps1" -OutFile "umber-app-installer.ps1"
Invoke-WebRequest -Uri "https://github.com/victorbessa96/umber/releases/download/v0.1.0-alpha.1/sha256.sum" -OutFile "sha256.sum"
$hash = Get-FileHash -Algorithm SHA256 umber-app-installer.ps1
$expected = (Get-Content sha256.sum | Select-String "umber-app-installer.ps1").Line.Split()[0]
if ($hash.Hash -ne $expected) { Write-Error "Checksum mismatch"; exit 1 }

# 2. Run installer (default prefix %LOCALAPPDATA%\Umber)
.\umber-app-installer.ps1 -Prefix "$env:LOCALAPPDATA\Umber" -AddToPath -CreateDesktopShortcut

# 3. Launch
umber-app
# (Same steps 4–7 as Linux; paths under %USERPROFILE%\umber-export\ etc.)
```

### 4.3 Expected File Sizes (Reference)

| Map | Resolution | Approx. Size (PNG, 2K) | Approx. Size (PNG, 4K) |
|-----|------------|------------------------|------------------------|
| AO | 2048² | ~800 KB | ~2.5 MB |
| ID Multi | 2048² | ~1.2 MB | ~4 MB |
| Ground Plane AO | 2048² | ~600 KB | ~2 MB |
| Ground Plane Height | 2048² | ~400 KB | ~1.5 MB |
| Ground Plane ID | 2048² | ~1 MB | ~3.5 MB |
| Skew | 2048² | ~200 KB (flat) | ~800 KB (flat) |
| Curvature | 2048² | ~1 MB | ~3.5 MB |
| Position | 2048² | ~2 MB | ~7 MB |
| Normal / WorldNormal / BentNormal | 2048² | ~1.5 MB | ~5 MB |
| Thickness | 2048² | ~800 KB | ~2.5 MB |

**Export bundle (glTF + textures, 2K, 2 sets, all maps)**: ~25–40 MB. **Well under 150 MB budget.**

---

## 5. Open Product Questions (Answer During Dogfood)

These are **product decisions** (not bugs) that Bessa must resolve based on the dogfood experience. Record answers in the session log.

| ID | Question | Context | Decision Needed |
|----|----------|---------|-----------------|
| **P1** | **Default cage method** | D2 offers Inflate / Shrinkwrap / Manual. Inflate is current default. Does it work for typical high→low workflows, or should Shrinkwrap be default? | Choose default + whether to expose method in UI at all (vs. hidden advanced). |
| **P2** | **Cage front/back offsets UX** | Current: two sliders (Front, Back) in High-poly group. Are these intuitive? Should they be "Min/Max ray distance" instead? | Rename / rescale / add presets (e.g., "Tight", "Loose"). |
| **P3** | **ID Multi — Material source gating** | Material source only works when per-triangle material assignment exists (not yet landed). Today it silently contributes nothing. Should the UI disable/hide it with a tooltip? | Hide vs. show disabled + explain; or keep as-is (advanced users know). |
| **P4** | **ID Multi — default blend mode** | Additive / Max / Replace. Additive is current default. Is this the most useful for "ID from multiple sources"? | Confirm default; consider "Replace" as more intuitive for layering IDs. |
| **P5** | **Ground plane — default maps** | Currently AO+Height+ID all enabled. Height is the displacement source; AO is contact shadow; ID is for masking. Should all three be on by default? | Choose default subset; maybe Height only (displacement driver). |
| **P6** | **Ground plane — size/height units** | Size in world units; height offset negative = below mesh. Is this clear? Should size be "relative to mesh bounds"? | Add "Fit to mesh" button; clarify units in tooltip. |
| **P7** | **Auto-rebake — 500 ms debounce** | Fixed 500 ms. Too fast? Too slow? Should it be configurable? | Keep fixed / expose slider / make it a power-user setting. |
| **P8** | **Bake log — retention** | Log persists in panel (not project file). Should it persist across sessions? Max entries? | Persist in `.umber` / limit to 100 / clear on new mesh. |
| **P9** | **Margin type — default** | Extend is default (Substance Painter parity). Adjacent Faces is Blender's "Dilate". Which default for Umber? | Confirm Extend default; document when to use Adjacent Faces. |
| **P10** | **Displacement preview — strength range** | Strength slider: 0.0–10.0? 0.0–1.0? World units? | Define range + default; add "Reset to 1.0" button. |
| **P11** | **First-run — sample mesh** | Installer doesn't ship a sample mesh. Should we bundle a simple two-material FBX (e.g., `cube_uv_ascii7400.fbx`) for the walkthrough? | Bundle sample / link to download / skip. |
| **P12** | **Export presets — baked map inclusion** | Current presets (Unreal/Unity/Blender) include base PBR channels. Should they auto-include baked maps (AO, Curvature, ID, etc.)? | Opt-in per preset / always include / separate "baked maps" preset. |

---

## 6. Battery Verification (Pre-commit)

Run before landing this brief and before the dogfood session:

```bash
python3 scripts/check-layers.py      # → "=== layers: GREEN (16 crates) ==="
python3 scripts/scorecard.py --check # → "scorecard: README.md current"
```

Both **must pass** (they do as of 2026-11-16).

---

## 7. Lock / Commit / Push Procedure

```bash
# 1. Add lock
bash scripts/kill-locks.sh add razul-dogfood-prep docs/loop/briefs/2026-11-17-dogfood-walkthrough-prep.md

# 2. Stage
git add docs/loop/briefs/2026-11-17-dogfood-walkthrough-prep.md

# 3. Commit with kill-lock release (path-limited)
dcommit --kill-lock razul-dogfood-prep --path-limited \
  --lock docs/loop/briefs/2026-11-17-dogfood-walkthrough-prep.md \
  -- -m "docs(umber): dogfood + first-run walkthrough prep brief (gates 8/C2-30)"

# 4. Remove lock
bash scripts/kill-locks.sh remove razul-dogfood-prep

# 5. Safe push (bare; sibling-ledger refusal OK)
safe-push

# 6. Verify
git log -1 --oneline
git status
# Both must be clean before reporting done.
```

**On ANY OS error 5**: STOP, stash, report.

---

## 8. Next Actions After This Brief Lands

1. **Bessa schedules dogfood** (C2-24) — runs the session plan in §3, fills the log, answers product questions in §5.
2. **Bessa runs first-run walkthrough** (C2-30) — follows §4.1/4.2 on a clean machine, verifies installer + export sizes.
3. **Dogfood findings** → separate kill report (`docs/loop/kill-reports/2026-11-XX-dogfood-findings.md`) with any P0 blockers.
4. **Product decisions** (§5 answers) → feed into D2–D5 follow-up kills (UI polish, defaults, persistence).
5. **Gate 8 MET** → Bessa calls Beta per release checklist phases.

---

**End of brief.** Ready for lock/commit/push.