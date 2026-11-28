# Kill report: EXR 16F and layered PSD production routes

- Locks: `razul-exr-psd-route`, `razul-psd-batch`, `razul-psd-agent`, `razul-psd-i18n`, `razul-psd-keyboard`, `razul-psd-bake-sources`.
- Trigger: late formats review findings 6 and 7. EXR 16F was documented but absent; the layered PSD writer existed only in tests.

## EXR 16F

- `OutputFormat::Exr16F` joins `Exr32F` with linear transfer semantics.
- `write_exr_f16` uses the vendored EXR crate's `f16` samples and preserves the existing top-first/bottom-up orientation.
- The driver widens u8 sources to linear f32 once, then emits F16 or F32 explicitly.
- Oracles assert F16 metadata on every channel, decoded half samples, buffer-size refusal, and the real `run_preset` path.

## Layered PSD

- `ExportPreset.psd` is a serde-default-off toggle; older preset files keep their old behavior.
- Single runs write `<textureSet>.psd`; tiled runs write one `<textureSet>_<tile>.psd` per map set. No dangling UDIM token.
- `expanded_output_paths` includes PSD, so duplicate detection and `ExportArtifactGuard` cover it.
- The app gains a localized Layered PSD checkbox and preset propagation, including `filter_satisfiable`.
- CLI gains `--psd`; batch recipes and agent export steps gain matching `psd` fields. All routes reach `write_psd_maps` through the driver.
- The keyboard journey now includes PSD in forward and backward focus order.

## Verification

- `cargo test -p umber-export -- --test-threads=1`: **122 passed, 0 failed**, plus **7/7** integration tests.
- `cargo test -p umber-app -p umber-cli -- --test-threads=1`: **931 app bin tests**, **123 CLI bin tests**, every integration target, **0 failed**.
- `cargo clippy -p umber-export -p umber-app -p umber-cli --all-targets -- -D warnings`: clean.
- Rustfmt, `git diff --check`, scorecard (**3,352**), layers, and parity: clean.
- Independent final review: `opencode/beam-free` **SHIP**, zero findings.

## Non-goals

No f32-source precision work beyond the existing u8-to-f32 conversion, no per-layer paint-layer PSD writer, no release/tag/upstream PR, and no change to PSD import semantics.
