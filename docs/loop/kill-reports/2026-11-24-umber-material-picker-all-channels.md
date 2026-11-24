# All-channel material picker R-A10

- Date: 2026-11-24.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-24-umber-material-picker-all-channels.md` (`ac11f655a1a0`).
- Claim: `7ccf37d05dc7`, queue row 586.
- Implementation: `663632c0aa55eab33c5893be9308ffcd495d2b46`.
- Final pre-commit source-diff SHA-256: `deede93692756a46f3b03eb33059ee0ff5bf0543036cea5527296bb765ad0955`.
- Kill lock: `razul-material-picker-all-channels`.
- Scout: `deleg_11326c15-1`.

## Built

- Added the session-only UV-footer `All channels` checkbox, default off. Ctrl+Alt+click remains the pick gesture in both modes.
- Active mode preserves the primary selected fill's channel/projection/mask and applies the sampled `Solid` through `SetFill`; non-fill or missing selections stay inert.
- All-channel mode enumerates `planes_present()` for the picked tile, deduplicates with baseColor first, and creates one new fill per materialized channel with default UV/unmasked params plus its sampled Solid and channel target. All N Add + N SetFill commands run inside one `run_grouped(AddFill)`, and the newest layer is selected and mirrored into `PaintState`.
- Every read is preflighted before mutation. A failed read aborts atomically; zero channels is inert. Success gets a count toast; failure gets a localized channel-named notice.
- `material_texel` samples per-channel dimensions with the mirrored-row convention and converts premultiplied bytes to straight linear RGBA, clamping to 1 and returning transparent black at alpha 0.
- Ctrl+Alt is now a first-class exclusive branch: it ends any in-flight stroke, records a typed `MaterialPick`, and suppresses vector/path/lasso/clone/paint handling.
- Added `uv.all-channels`, `uv.pick-applied-count`, and `uv.pick-failed` to both i18n catalogs.

## Review and dispositions

- R1 `deleg_e9845b8b`: **REQUEST_CHANGES**, two valid findings.
  - **P1:** mirror/path/vector handles were processed before the exclusive Ctrl+Alt branch. Fixed by evaluating raw Ctrl+Alt+primary click before ANY handle widgets are created, so previous-frame hit-test occlusion cannot steal the chord. The in-flight stroke still ends before the return.
  - **P2:** the e2e armed only polygon. It now arms mirror, polygon, path, and vector and asserts zero leaked captures. A separate `material_pick_footer_toggle_is_live` proves the checkbox independently of the armed fixture.
- R2 `deleg_80131eb1`: **SHIP**, zero findings. It independently confirmed raw Ctrl+Alt+primary input runs before paint, mirror, path, or vector interaction widgets, previous-frame handle occlusion cannot suppress the pick, the armed fixture creates no leaked captures, and the separate footer toggle is live.

## Verification

- RED: `cargo test -p umber-app --bin umber material_pick -- --nocapture` failed all three initial acceptance tests: straight-alpha conversion, grouped all-channel creation, and second-read failure-atomicity.
- GREEN after R1 fixes: the same filter now passes **5/5**: the three initial tests plus the footer toggle and the real `UvView::ui` → `take_pick` → `apply_material_pick` e2e. The e2e creates baseColor + a layer-owned custom channel, ends the in-flight stroke, creates no lasso/path/vector capture, preserves the source layer, selects/mirrors the newest fill, and proves one-step undo/redo.
- Strict `cargo clippy -p umber-app --all-targets -- -D warnings` → exit 0 after removing one needless shell reborrow.
- Full `cargo test -p umber-app` after R1 fixes with `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`: **1,271 passed, 0 failed across 11 suite rows**: 323 lib + 901 bin + 2 filter-apply + 1 Gate0 + 3 keyboard-menu + 17 panel-kittest + 11 snapshots + 10 panic-hunt + 2 pool-perf + 1 set-switch + 0 doc-tests.
- `~/.cargo/bin/rustfmt --edition 2021 --check` on touched Rust → exit 0.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- A fresh OS-safe `hermes-verify-material-picker-*.sh` probe reran the exact picker filter after R1 fixes: **5/5**, `ADHOC-VERIFY: PASS`, script deleted and verified absent. This is focused ad-hoc evidence in addition to the app suite, not a workspace-green claim.

## Judgment calls

- The additive N-fill branch of the original derivation was chosen because it is non-destructive and needs no channel-set identity/reuse convention. In-place reuse remains a separate product question.
- No `FillParams` change was made: one source and one target channel per fill is preserved.
- Materialized-but-undeclared and layer-owned channels are included; declared-but-unmaterialized channels are never allocated by a pick.
- No display LUT, grayscale coercion, range remap, normal normalization, or DirectX/OpenGL Y-flip is applied.
- A size override is used in the e2e fixture so the layer-owned 2x2 custom channel is accepted without creating a Base custom plane.

## Unverified

- No dedicated interactive application-window smoke was run. The real headless UV view, paint/composite readbacks, grouped journal, i18n catalogs, and full app/integration suites carry the evidence. No Windows run, full workspace test, or fresh CI dispatch was performed. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Next independent kill: the Assets browser mesh category. Then OccluderScope threading kittest and XI2 hotplug re-query.
- A temporary Painter-style `P` tool, 3D-viewport material pick, channel-set identity/reuse convention, and multi-channel `FillParams` remain out of scope.
