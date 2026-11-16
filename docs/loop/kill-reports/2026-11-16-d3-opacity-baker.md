# Kill report: D3 opacity baker (kill `razul-d3-opacity-baker`)

- **Date**: 2026-11-16
- **Commit**: `c499255ff9d5` (master)
- **Brief**: `docs/loop/briefs/2026-11-16-d3-opacity-baker.md` (a076aeef)
- **Lock**: `razul-d3-opacity-baker` (7 files + kill report + queue row)

## Files changed

| File | Kind | Lines |
|------|------|-------|
| `crates/umber-bake/src/opacity.rs` | NEW | ~860 |
| `crates/umber-bake/src/lib.rs` | MOD | +8 |
| `crates/umber-gpu/src/bake_shaders.rs` | MOD | +270 |
| `crates/umber-app/src/bakes_panel.rs` | MOD | +120 |
| `crates/umber-mesh/src/mesh_maps.rs` | MOD | +5 |
| `crates/umber-gpu/src/view_mode.rs` | MOD | +10 |
| `crates/umber-app/src/mesh_map_import.rs` | MOD | +1 |
| `assets/i18n/en-US.ftl` | MOD | +8 |
| `assets/i18n/pt-BR.ftl` | MOD | +8 |
| `docs/loop/queue.md` | MOD | +2 |

## Battery results

| Check | Result |
|-------|--------|
| `rustfmt --edition 2021 --check` | **GREEN** (all touched .rs) |
| `python3 scripts/check-layers.py` | **GREEN** (16 crates) |
| `python3 scripts/scorecard.py --check` | **GREEN** |
| `cargo clippy -p umber-bake -p umber-gpu -p umber-app` | **GREEN** (0 warnings on modified crates; pre-existing `unused-doc-comments` in `umber-mesh` unrelated) |
| `cargo test -p umber-bake -p umber-gpu -p umber-app --lib` | **0 failed** (106 + 156 + 319 passed) |

## Kittest-first DoD (all PASS)

| Test | Description | Status |
|------|-------------|--------|
| `opacity_cavity_matches_ao_grayscale` | `OpacitySource::Cavity`, `strength=1.0`, `invert=false` → R channel == AO R channel (AO=1-occlusion; opacity=occlusion=1-AO) | PASS |
| `opacity_id_map_masks_coverage` | Bake ID map first, then opacity with `OpacitySource::IdMap`, `strength=1.0` → covered texels opacity=255, uncovered=0; `invert=true` flips | PASS |
| `opacity_manual_uniform` | `OpacitySource::Manual`, `strength=0.37` → covered R=95 (0.37\*255 rounded), A=255; uncovered (0,0,0,0) | PASS |
| `opacity_off_is_byte_identical_to_today` | `BakeSelection::Opacity` unchecked → no new jobs, no new log entries, no new files (plan_bake test) | PASS |
| `opacity_mesh_fed_matches_plane` | Mesh-fed double-bake byte-identical (same ray count, seed order, IEEE arithmetic) | PASS |

## Deviations from brief

1. **Plane path `cs_main`**: The plane entry point only supports `Cavity` source (as specified). The brief said IdMap/Manual "not supported in plane path (mesh-fed only)" — implemented as early return for non-Cavity sources. The plane path uses a simplified fixed plane at origin rather than the full `PlaneDesc` uniform; the mesh-fed path is the primary one.

2. **Clippy on `umber-mesh`**: Pre-existing `unused-doc-comments` warnings (4 locations) in `crates/umber-mesh/src/lib.rs` cause the full 4-crate clippy to fail. These are unrelated to D3 changes. The modified crates (`umber-bake`, `umber-gpu`, `umber-app`) are clean.

3. **Push refused**: `safe-push.py` refused because a sibling session's commit (28f6c467) is ahead on origin. The project-watch cron lane will push both. Local master is at `c499255f`; origin is at `d49d0acc`.

## Verification

- `git ls-remote origin master` → `d49d0acc68ac25b359cb611fdd864e4a1d8b35ab`
- Local tip: `c499255ff9d5` (D3 opacity baker)
- Queue row appended: `docs/loop/queue.md` entry 495
- Kill lock released: `razul-d3-opacity-baker`