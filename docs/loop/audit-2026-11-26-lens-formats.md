# Formats lens: late-review reconciliation (2026-11-27)

Review source: `deleg_320688ac-2`, targeting `ec979743`. Reconciled against the current tree, not treated as a fresh diff. Four findings were already fixed at reconciliation time; three were live and are now closed.

## Finding dispositions

| # | Disposition | Current evidence |
|---|---|---|
| 1 — MaterialX/USD filename escape | **Stale: already fixed** | `driver::document_output_path` applies `path_safe_value` and rejects symlink/non-regular destinations. `write_materialx` and `write_usda` both use it. |
| 2 — PNG16 native-endian samples | **Stale: already fixed** | `write_png16` serializes every sample with `to_be_bytes`; the test asserts the encoded bytes and decodes with `u16::from_be_bytes`. |
| 3 — Multi-set USDA references the wrong files | **Stale: already fixed** | The app builds per-group/per-tile sources and documents, so Body/Hilt files and references agree. |
| 4 — Expanded collisions + partial exports | **LANDED** | `expanded_output_paths[_tiled]` preflights duplicates, `ExportArtifactGuard` backs up destinations and restores/removes on failure, MTL compatibility is checked before textures, and app/CLI outer guards span the whole run. Report: `kill-reports/2026-11-27-export-transaction.md`. |
| 5 — PSD fallback alpha math | **Stale: already fixed** | `over_pixel` uses straight-alpha source-over with `dst_a` and `out_a` normalization before the publisher's premultiply. Existing transparent/partial-underlay tests cover it. |
| 6 — EXR 16F missing | **LANDED** | `OutputFormat::Exr16F` and `write_exr_f16` now produce real F16 channels through the preset driver, with metadata and decoded-sample oracles. Report: `kill-reports/2026-11-27-exr-psd-route.md`. |
| 7 — Layered PSD has no production route | **LANDED** | `ExportPreset.psd` drives one layered PSD per map set/tile through single/tiled runs, the app checkbox, CLI `--psd`, batch recipes, and agent exports. Report: `kill-reports/2026-11-27-exr-psd-route.md`. |

## Closed rows retained from the review

- OBJ/MTL and GLB remain closed as scoped. Finding 4 was a transaction issue, not an encoding regression.
- SVG import/export remains closed; its direct save shares the general transaction boundary rather than a new SVG-specific defect.
- Canonical mesh-extra paths remain closed with first-wins ordering and duplicate suppression.
- SPP multi-chunk decoding and R6 checksum reporting remain closed. R7 stays detection/report-only by design.
- SPP R1–R4 and PSD per-layer/text recovery remain explicit priority-gated boundaries, not new findings.

## Verification and review

- `cargo test -p umber-export -- --test-threads=1`: **122 passed, 0 failed**, plus **7/7** integration tests.
- `cargo test -p umber-app -p umber-cli -- --test-threads=1`: **931 app bin tests**, **123 CLI bin tests**, every integration target, **0 failed**.
- Strict export/app/CLI clippy, rustfmt, scorecard (**3,352**), layers, parity, and `git diff --check`: clean.
- Final independent review: `opencode/beam-free` **SHIP**, zero findings.
