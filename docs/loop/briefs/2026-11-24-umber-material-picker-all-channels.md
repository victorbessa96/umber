# Design brief: all-channel material picker R-A10 (2026-11-24)

Planned implementation lock: `razul-material-picker-all-channels` (the implementation session must take the full source lock).
Current brief-only lock: `razul-material-picker-brief`.
Base: `8e30c506ce0b692f3da9fcad66fcabf95e6c3782`.
Scouts: `deleg_11326c15-1`, with the earlier channel-target and v1 reports at `docs/loop/kill-reports/2026-10-24-fill-channel-target.md` and `2026-10-24-material-picker-v1.md`.

## Current behavior and defects

- Ctrl+Alt+click reads only `paint.active_channel()` and writes that composite texel into the primary selected fill layer's `FillSource::Solid`, preserving its channel/projection/mask through `SetFill` (`crates/umber-app/src/main.rs`).
- `FillParams` is deliberately one source plus one target channel (`crates/umber-mesh/src/fill.rs`). It is not extended into a multi-channel model.
- The current plain `byte / 255` conversion is wrong for translucent samples: `RgbaBuffer` is premultiplied, while `FillSource::Solid` expects straight linear RGBA.
- The current pick is not exclusive: after recording Ctrl+Alt+click, the UV view can still fall through to polygon/path/vector/clone/paint handling.
- `channels_present()` covers only Base target channels and misses layer-owned fills/materials. `planes_present()` is the required census.

## Target contract

### UI and mode

- Add one session-only UV-footer checkbox `All channels`, default off, beside the existing isolate control. Ctrl+Alt+click remains the pick gesture in both modes. Do not use Shift+Ctrl+Alt: Shift already owns the UV pixel inspector.
- Off: preserve v1's primary-selection and `SetFill` behavior. A non-fill selection or `fill: None` stays a logged no-op.
- On: require no source selection and modify no existing layers. Create one new fill layer per materialized channel, with default UV/unmasked `FillParams` except its sampled `Solid` and target `channel`. Select the last new layer and mirror that target into `PaintState` using ordinary add-then-select conventions.
- Put all N `Add` + N `SetFill` commands inside `Document::run_grouped(HistoryKind::AddFill, ...)`: one undo step and one history row, with standard numbered fill names and default layer properties. This chooses the additive, non-destructive N-fill branch of the original derivation; in-place channel-set identity/reuse remains a separate product question.

### Enumeration and values

- Enumerate `paint.planes_present()`, filtered to the picked tile and deduplicated, with `baseColor` first and remaining names sorted. Include materialized-but-undeclared channels and layer-owned planes. Do not materialize declared-but-absent channels.
- Sample every channel at the same UV using that channel's own dimensions, clamped, with the existing mirrored-row convention. A per-channel-size override must not change the sampled UV.
- Un-premultiply sampled RGB by alpha, clamp to 1, and return zero RGB at alpha 0. Preserve the sampled alpha. No display LUT, additional sRGB transfer, grayscale coercion, range remap, normal normalization, or DirectX/OpenGL Y-flip.

### Gesture exclusivity

Make Ctrl+Alt+click a first-class branch that records the pick, ends any in-flight stroke, and suppresses vector, path, lasso, clone, and paint handling for that chord. The test must prove no stroke and no lasso/path capture.

### Failure and feedback

- Preflight every read before any document mutation. Any readback failure aborts the whole all-channel pick with no layer/history edits and a localized failure notice.
- Zero discovered channels is also a no-op with no empty history group.
- Successful all-channel picks get a count toast; active picks retain the existing hex toast. No status-bar change is required.
- Later fill-refresh budget failures remain under the existing fill badge/retry lifecycle rather than becoming picker transactions.
- Add only the footer label and count/failure messages to both i18n catalogs; no new status-bar hint or icon is needed.

## Implementation seam

Extract one production `apply_material_pick` helper used by the shell's `Panel::UvView` arm. Keep read sampling, straight-alpha conversion, materialized-only enumeration, grouped creation, and failure preflight in that helper so the production path and tests cannot drift.

The most direct e2e path is `#[cfg(test)] mod material_pick_tests` in `main.rs`: production `UvView::ui` → `take_pick` → the same helper. Reuse the existing headless Harness and paint-capable device helper; do not construct a full `UmberApp`.

One compact fixture must cover:

- 4x4 baseColor and 2x2 roughness with different sentinels at the same UV, proving per-channel dimensions and V mirroring.
- A layer-only normal/custom channel, proving `planes_present()` enumeration.
- A declared but unmaterialized channel, proving no allocation/default fabrication.
- Distinct RGBA values, including one translucent texel, proving straight-alpha conversion.
- A selected fill with non-default projection/mask for the active-mode preservation leg.
- An armed polygon tool, proving the pick does not also capture a lasso or stroke.

Assertions: exact created channel/value pairs, default projection/mask on new fills, existing layers untouched in all-channel mode, last-layer selection, no new channel materialization, no stroke/capture, one undo removing the whole picked set, and redo restoring the same ids. Add a reader-injected unit test that fails on the second channel and proves no document/history mutation, plus the existing non-fill active-selection no-op.

## Concurrency and safety

The implementation session must lock the intended surface before its first source edit: `crates/umber-app/src/main.rs`, `uv_view.rs`, `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`, this brief/report, `docs/ROADMAP.md`, `docs/loop/waves/2026-11-09-wave-plan.md`, and the current status/queue surfaces. Do not alter `FillParams`, the refresh evaluator, schema, or dependencies. Any foreign dirty file, live sibling lock, origin movement, disk I/O error, or unexpected behavior delta stops the lane for re-census.

## Verification and acceptance

- RED first: prove translucent conversion is wrong and the all-channel mode is absent before implementation. Then run the exact `cargo test -p umber-app --bin umber material_pick -- --nocapture` filter and require a nonzero passed count.
- Run strict `cargo clippy -p umber-app --all-targets -- -D warnings` and full `cargo test -p umber-app` separately under `/tmp/umber-cargo.lock` with the slow-build environment and `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`. Record every suite's passed/failed count; 0-test success is not a pass.
- Run rustfmt, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Obtain independent review of materialized-only enumeration, straight-alpha math, per-channel UV sampling, gesture exclusivity, grouped undo/redo, selection mirroring, and failure-atomicity.
- Record the kill and update the current remaining-work order, commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`.

## Non-goals

No temporary Painter-style `P` tool, 3D-viewport material pick, channel-set identity/reuse convention, multi-channel `FillParams`, new projection model, or async fill-refresh transaction. Assets mesh category, OccluderScope kittest, and XI2 hotplug remain later independent kills.
