# Brief — 2026-11-05 assets panel + starter content wave (workflow priority #2, the roadmap's named moat gap)

Written 2026-11-05 by deleg_c0cb350a, design-only (queue row 179b). Sources:
`docs/loop/workflow-2026-11-05-parallel-lanes.md:47` (priority #2),
`docs/ROADMAP.md:222` (Assets/ecosystem = **GAP — the moat**),
`docs/research/spp-parity-census-2026-10-16.md:43-48` (C1/C2), the rc1 brief
`docs/loop/briefs/2026-10-24-rc1-assets-panel.md`, and a live survey of
`crates/umber-app/src/{assets_panel,asset_thumbs,env,document,main,layers_panel,first_run,empty_state}.rs`,
`crates/umber-cli/src/main.rs`, `crates/umber-core/src/smart.rs`,
`crates/umber-brush/src/preset_library.rs`, `assets/`,
`crates/umber-app/tests/panel_snapshot.rs`. No code changed; this file is the only commit.

## 1. Survey — what exists today (verified live)

**The panel.** `Panel::Assets` arm at `main.rs:1219-1273` drives
`assets_panel::library_ui` (`assets_panel.rs:137-341`): a persisted search field
(`:50`, restore/persist at `:69-95`), an "Extra library folders" manager
(`:157-208`, persisted via `document::extra_dirs_path`/`load`/`save` at
`document.rs:1112-1160`, scan-epoch-keyed caches), and three collapsible
categories — Brushes (`:213-232`, rows from `brush.library.entries`,
apply-on-click via `BrushPanel::select_preset_by_name` `brush_panel.rs:505`),
Smart materials (`:234-288`, `document::list_smart_materials`
`document.rs:1189`, apply via `document::apply_smart_material` `:1387` →
`main.rs` `apply_library_material`), Smart masks (`:290-338`,
`list_smart_masks` `:1223`, compose via the Layers panel's
`smart_mask_applied` flow). Material/mask rows carry content-keyed swatches
(`asset_thumbs.rs`, `THUMB_PX=32` `:21`, `mask_preview` `:101`,
`material_preview` `:124`, `thumb_key` `:184`) and an egui drag payload
(`AssetDrag(AssetDropRef::{Material,Mask})`, `layers_panel.rs:635-646`) with
drag-out-to-disk handled at `main.rs:2647 poll_asset_drag_out`. Below the
library, an Import… window (`main.rs:1258-1272`, `assets.import_open`) holds
the mesh-map / PSD / spp import sections.

**The libraries on disk.** `assets/smart/` = 2 `.umbersmart` + 1 `.umbermask`
(16 KB); `assets/brushes/` = 13 `.umberbrush` (52 KB) scanned by
`preset_library::load_from_dirs`/`load_default_dirs` (`preset_library.rs:79,:121`;
user dir + exe-dir `brushes/`, repo `assets/brushes` added dev-side);
`assets/env/` = 1 procedural equirect `studio-neutral.png` 1024×512 (105 KB,
generated deterministically by `assets/gen/make_studio_env.py`);
`assets/fonts/` = Inter + JetBrains Mono TTF (1.2 MB) **with their OFL texts
committed** (`assets/fonts/OFL-*.txt`). `document::smart_dirs()`
(`document.rs:1164-1182`) = repo `assets/smart` + exe-dir `smart/` + persisted
extras, first-dir-wins dedup — so starter content is visible on first run with
no state dir at all.

**Env loaders exist but are not in the panel.** `env.rs:16`
`DEFAULT_ENV_ASSET`, `:33 load_environment_file` (PNG LDR / EXR HDR gate at
`:20-27`), `:66-97 default_env_candidates`/`load_default_environment` — wired
only to boot (`main.rs:2757`) and File ▸ Load Environment… (`main.rs:2773-2782`).

**Tests.** `assets_panel.rs` kittests `:383-428` (bundled presets listed +
＋-applies "Painted wood"; search filters) and `:435-459` (search persistence);
`asset_thumbs.rs` unit tests `:195-306`; `tests/panel_snapshot.rs` = **8 snapshot
tests** (`:23,48,77,88,108,120,143,168`) — the snapshot pin this wave must keep
at 8/8; i18n keys `assets.*` in `assets/i18n/en-US.ftl:992-1001` (both
catalogs `en-US.ftl`/`pt-BR.ftl` required).

**CLI.** `crates/umber-cli/src/main.rs:37-46` — `inspect|bake-ao|bake-all|
export|batch|agent|mcp|serve|extract-spp|bench`. No library list/import
command exists.

## 2. Gap list vs the roadmap moat (census C1/C2, file:line evidence)

1. **Starter content is 3 smart assets vs R-C2's 30-50** (census
   `spp-parity-census-2026-10-16.md:48`; `assets/smart/` = 3 files, 16 KB).
   The moat row `ROADMAP.md:222` is unaddressed by content, only by browser.
2. **Env-map category absent from the panel** although the loader exists
   (`env.rs:33`); census C1 names env maps in the library
   (`census:46`). Panel has brushes/materials/masks only
   (`assets_panel.rs:213-338`).
3. **No mesh or filter categories**: no mesh-library loader (mesh lane owns
   meshes; one mesh per session in v1) and **no filter asset type at all**
   (`umber-core/src/smart.rs` defines only `SmartMaterial`/`SmartMask`/
   `AnchorRef`, `:43-556`).
4. **No saved searches** — only one persisted free-text query
   (`assets_panel.rs:50,:69-95`); census C1 names saved searches.
5. **No category filters beyond free text** — a single `matches` closure
   (`assets_panel.rs:151`); and when the search filters a non-empty list to
   zero rows there is **no "no matches" state** — the `assets.empty` label
   fires only when the library itself is empty (`:229-231,:286-288,:336-338`).
6. **Brush rows are text-only**: no swatch (materials/masks get thumbnails at
   `:263,:311`; brushes `:221-227` have none, though `.stamp.png` tips exist in
   the brush shelf) and no drag (`AssetDropRef` = Material|Mask only,
   `layers_panel.rs:635-641`).
7. **No CLI library surface**: `umber-cli/src/main.rs:37-46` has no
   list/import-into-library command — the headless-first moat can't see the
   starter content.
8. **No license inventory under `assets/`**: fonts carry OFL texts
   (`assets/fonts/OFL-*.txt`), but `assets/smart`, `assets/brushes`,
   `assets/env` have no committed license note.

## 3. Numbered kills per lane

Lane map per `workflow-2026-11-05-parallel-lanes.md:5-14` (app lane owns
`crates/umber-app/src/*` + `assets/i18n`; cli lane owns `crates/umber-cli`;
the assets/ content lane rides the docs/scripts lane's `assets/` ownership —
`assets/fonts` explicitly, the new starter dirs likewise). Every kill is
behavior-preserving (additive UI, additive CLI commands, additive files) or
snapshot-pinned: **`tests/panel_snapshot.rs` stays 8/8 green** and new surfaces
get their own kittests instead of touching existing snapshots.

**App panel lane**
- **K1 — Environment maps category.** New `list_env_maps(dirs)` loader in
  `env.rs` (same first-dir-wins/skip-bad-file rules as
  `document::list_smart_materials`, `document.rs:1189-1221`) over
  `assets/env` + exe-dir `env/` + the persisted extra dirs. Fourth collapsible
  category in `assets_panel.rs` (default **closed**, so no existing snapshot
  moves); row click returns `LibraryOutcome.env: Option<PathBuf>` and the
  shell reuses the exact File▸Load path (`main.rs:2780`:
  `env::load_environment_file` → `viewport.set_environment`). Swatch =
  downsampled equirect thumbnail via `asset_thumbs`.
- **K2 — No-match state + brush swatches.** Distinct `assets.no-matches`
  label when the search empties a non-empty list (empty-library
  `assets.empty` unchanged); brush rows get `.stamp.png`-keyed thumbnails
  through the same `ThumbKey` cache (`assets_panel.rs:99-119`).
- **K3 — Saved searches.** Named searches persisted in a new state-dir file
  `assets_saved_searches.json` next to the existing single-query file, which
  stays read-compatible (`:69-95` untouched); collapsible "Saved searches"
  section with save-current / apply / delete.
- **K4 — First-run empty-state wiring.** `first_run.rs:66-68 should_show`
  (no prefs file) already implies a fresh state dir → `smart_dirs()`
  (`document.rs:1164`) scans the bundled starter dirs with zero state, so the
  browser is non-empty on day one; the kill adds the diagnostic half — when a
  category's list is empty *and* no starter dir was found, log a warn (the
  env loader's `env.rs:95` precedent) — plus a home-card (`empty_state.rs`)
  "Browse starter materials" button that switches to `Panel::Assets`
  (`main.rs:207`). Kittest: a fresh-state `AssetsPanel` lists the bundled
  brush/material/mask/env rows (extends the existing kittest at
  `assets_panel.rs:383-414`).

**CLI import lane**
- **K5 — `umber-cli import-asset`.** New command
  `import-asset <file> --kind <smart-material|smart-mask|brush|env>
  [--library DIR] [--dry-run]` (`main.rs` command table + new module): validates
  with the *same* loaders the GUI uses (`umber_core::smart::*::load_file`,
  `umber_brush` preset parse, `env::format_for_path` gate), refuses invalid
  files typed (never a silent copy), then copies bytes-untouched into the
  per-user library dir (brush `user_preset_dir`, `preset_library.rs:129+`;
  state-dir `smart/`; env dir) or `--library DIR`. `--dry-run` prints the
  resolved target + validation verdict. Unit tests with fixture files.
- **K6 — headless library listing.** Agent step + MCP tool `library.list`
  (the row-154 `command.list` precedent) over the same scan fns, so the
  starter + user libraries are inspectable headless; `agent.rs` step
  vocabulary gains `import_asset` delegating to K5.

**assets/ content lane**
- **K7 — starter smart library, first slice.** Author 8 original
  `.umbersmart` + 4 original `.umbermask` (→ 10 materials + 5 masks total),
  each pinned by a loader round-trip test; every row gets its swatch for
  free via the content-keyed cache. The R-C2 30-50 full target stays on
  wave-community-templates (`ROADMAP.md:164`); this wave ships the credible
  starter slice.
- **K8 — starter env set.** Extend `assets/gen/make_studio_env.py` to emit 2
  more deterministic procedural variants (warm key, cool fill) at 1024×512
  LDR PNG; commit the 3 PNGs (~105 KB each). EXR deferred (multi-MB, see §4).
- **K9 — license inventory.** `assets/LICENSES.md`: fonts = OFL 1.1 (texts
  already in `assets/fonts/OFL-*.txt`), brushes/smart/env = original,
  authored in-repo, CC0; env PNGs procedural via `make_studio_env.py`
  (deterministic, no licensed content — the generator's own docstring).

## 4. Starter content plan

- **Ships by default**: 13 brushes (`assets/brushes`), 10 smart materials +
  5 masks (`assets/smart` after K7), 3 env maps (`assets/env` after K8), the
  two OFL fonts. All are scanned with zero user state (`document.rs:1164`,
  `preset_library.rs:121`, `env.rs:66`) — first run (`first_run.rs:66-68`)
  shows a populated browser, and the panel's empty state (K4) only fires on a
  genuinely empty library.
- **License notes**: every shipped asset is original or OFL; the inventory
  lands as `assets/LICENSES.md` (K9). No .sbsar, no Adobe-derived content
  (the `ROADMAP.md:200-204` legal ruling stands).
- **Size budget**: the §12 release budget is < 150 MB tracked
  (`docs/specs/installers-design.md:23-27`); the whole `assets/` tree is
  ~1.6 MB today and this wave adds **< 500 KB** (target: starter smart/brush/env
  payload under 1 MB). Env maps stay 1024×512 LDR PNG; no committed EXR.
- **First-run empty-state wiring**: `first_run.rs` Quick Setup needs no new
  step — the starter dirs are state-independent; K4 adds the missing-diagnostic
  warn and the home-card cross-link so "empty tool" is never what a fresh
  install feels like (the census's P0 rationale, `census:47`).

## 5. Verification battery per kill

- Per-crate, under `flock /tmp/umber-cargo.lock` with the workflow's env
  (`CARGO_BUILD_JOBS=3`, debug=0): `cargo fmt`,
  `clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`.
- **Snapshot pin**: `cargo test -p umber-app --test panel_snapshot` stays
  **8/8**; K1's env category ships default-closed so no existing snapshot
  moves; new panel surfaces get their own kittests in `assets_panel.rs`'s
  `kittest` module (the existing `:343-429` pattern), never edits to the 8.
- K1: kittest lists `studio-neutral.png` (+K8's variants) and a row click
  yields the env outcome; env loader unit tests extend `env.rs:99-191`.
- K2/K3: kittests for no-match label, brush swatch presence, saved-search
  save/apply/delete round-trip through a temp state file.
- K4: kittest that a fresh-state panel lists bundled rows in all four
  categories; `first_run.rs` tests (`:256-298`) unchanged and green.
- K5/K6: cli unit tests — valid fixture copies into the target library dir,
  invalid file refused typed, `--dry-run` writes nothing; `library.list`
  output pinned against `assets/` fixture; MCP catalog pin per row 155's
  catalog-identical rule.
- K7/K8/K9: loader round-trip test per new `.umbersmart`/`.umbermask`;
  `make_studio_env.py` re-run produces byte-identical PNGs (its determinism
  contract); a docs-lane test asserting every file under `assets/smart`,
  `assets/brushes`, `assets/env` has an extension the loaders accept and a
  license line in `assets/LICENSES.md`.
- i18n: every new `assets.*` key in **both** `en-US.ftl` and `pt-BR.ftl`
  (the battery's i18n-completeness leg).
- Commit per kill: `git add <lane paths>` +
  `python3 .dragon/dcommit.py --path-limited --lock <paths> -- -m "..."`;
  never bare commit, never push (dragon owns the wave push).
- One ad-hoc `hermes-verify-*` script per code turn under the scratch dir,
  labeled ad-hoc (the verification hook).

## 6. Risks + non-goals

- **Snapshot drift**: any change to the existing three categories' layout
  risks the 8/8 pin — new categories ship default-closed and new rows only
  append. The assets panel currently has no snapshot test of its own; adding
  one (9th) is allowed but must be additive.
- **`AssetDropRef` extension** (a future Brush/Env drag variant) touches
  `layers_panel.rs:635-646` drop logic — out of this wave; drag-out stays
  Material/Mask (`main.rs:2647`).
- **/tmp is a 7.8 GB tmpfs that hit 100% on 2026-11-05** — check
  `df -h /tmp` before any battery (workflow doc's standing warning).
- **Lane conflicts**: app lane must not touch `crates/umber-cli` or
  `assets/smart|brushes|env` content; content lane must not touch app sources;
  `assets/i18n` rides the app lane.
- **License risk**: only original/procedural content ships; the OFL texts
  stay next to the fonts; K9's inventory is the audit hook.
- **Non-goals**: no .sbsar execution or Adobe-derived assets (legal ruling);
  no mesh-library category (mesh lane, one mesh per session v1); no filter
  format (no type exists — `smart.rs`); no marketplace/distribution (R-C4);
  no changes to the Import… dialog's three sections; no saved-search cloud
  sync (local-first); the R-C2 30-50 full library stays on
  wave-community-templates; this wave is design+starter slice only, and no
  push — dragon owns the wave close.
