# Assets browser mesh category

- Date: 2026-11-24.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-24-umber-assets-mesh.md` (`9935d582aa82`).
- Claim: `752c32ab4fa0`, queue row 588.
- Implementation: `6ef031944f8b3333d4bc50c440c9c96739209973`.
- Final pre-commit source-diff SHA-256: `a77001dbc2e2b45d38d62c14a690fb9b5798831be2814857dce14c59f97c35b0`.
- Kill lock: `razul-assets-mesh`.
- Scout: `deleg_11326c15-2`.

## Built

- Added lazy `mesh_dirs`/`mesh_dirs_with` and `list_meshes`: bundled `assets/meshes`, executable-side `meshes/`, then persisted extras with canonicalized first-wins directory dedup.
- All eight recognized extensions are accepted case-insensitively through `detect_format`: OBJ, GLTF/GLB, FBX, USD/USDA, USDC, and USDZ. Direct files only, sorted by stem, first-directory-wins filename dedup. No eager parse during scanning; malformed recognized files remain rows and fail only on activation.
- Added `LibraryOutcome.mesh`, `THUMB_KIND_MESH=5`, and a deterministic generic `mesh_glyph` with no file I/O.
- Added the sixth default-closed `Meshes` category with epoch-cached listing, search/no-match behavior, full-path hover, AccessKit labels, and click outcome.
- Added `mesh_loads` through the existing dock-drain pattern; `UmberApp` applies it through the exact `load_mesh_path` transaction after the dock. No second importer or alternate GPU path.
- Added `assets.meshes` to both catalogs and the bilingual key pin.

## Review and dispositions

- R1 `deleg_915dd909`: **REQUEST_CHANGES**, two valid findings.
  - **P2:** `Some(empty)` was treated as a cache miss, so an empty Meshes scan reloaded extras and directories every frame. Fixed by matching cache-entry presence: `Some(empty)` now stays cached until `scan_epoch` changes. The new regression creates a file after the first empty scan and proves it stays invisible until the epoch bump.
  - **P2:** the lister test omitted the missing-directory case. It now includes a nonexistent directory, while malformed recognized files still list without parsing. The full mesh filter passes 32/32 including the cache regression.
- R2 `deleg_86d758c2`: **SHIP**, zero findings. It independently confirmed `Some(empty)` stays cached until `scan_epoch` changes, the new file regression appears only after the epoch bump, the missing-directory case is covered, and malformed recognized files still list through extension-only detection.

## Verification

- RED: `mesh_dirs_preserve_bundled_exe_and_extra_order_without_duplicates` failed with extras-only output instead of bundled + exe + one canonical extra.
- GREEN: directory test 1/1 and full lister 1/1, covering all eight extension forms, mixed case, sorting, first-wins filename dedup, unsupported-file rejection, and malformed recognized files listed without parsing.
- Category kittest 1/1: header default closed, malformed OBJ lists after opening, `THUMB_KIND_MESH` cache entry exists, search filters the row with a no-match state, and clicking returns the exact path.
- Focused mesh filter after R1 fixes: **32/32**, including the glyph test and `empty_mesh_results_stay_cached_until_scan_epoch_changes`.
- Strict `cargo clippy -p umber-app --all-targets -- -D warnings` → exit 0.
- Full `cargo test -p umber-app` after R1 fixes with `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`: **1,276 passed, 0 failed across 11 suite rows**: 323 lib + 906 bin + 2 filter-apply + 1 Gate0 + 3 keyboard-menu + 17 panel-kittest + 11 snapshots + 10 panic-hunt + 2 pool-perf + 1 set-switch + 0 doc-tests.
- `~/.cargo/bin/rustfmt --edition 2021 --check` on touched Rust → exit 0.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- A fresh OS-safe `hermes-verify-assets-mesh-*.sh` probe reran the mesh filter after R1 fixes: **32/32**, `ADHOC-VERIFY: PASS`, script deleted and verified absent. This is focused ad-hoc evidence in addition to the app suite, not a workspace-green claim.

## Judgment calls

- Recognized-but-corrupt files remain visible because parsing belongs to activation. A scan never opens or parses a mesh candidate, avoiding eager 1 GiB imports and turning malformed rows into honest activation failures instead of disappearing silently.
- The thumbnail is explicitly a generic glyph, not a geometry preview. Content-accurate mesh rendering remains a separate asynchronous follow-up.
- The mesh click travels through `mesh_loads` so `UmberApp`, not `PanelViewer`, owns the full import transaction and its failure boundaries.

## Unverified

- No dedicated interactive application-window mesh-load smoke was run. The lazy lister, category kittest, post-dock rail, and full app/integration suites carry the evidence. No Windows run, full workspace test, or fresh CI dispatch was performed. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Next independent kill: OccluderScope combo → request → resolution kittest, then XI2 hotplug re-query.
- Content-accurate mesh thumbnails, asynchronous mesh preview rendering, starter meshes, drag/drop mesh outcome, and alternate import paths remain out of scope.
