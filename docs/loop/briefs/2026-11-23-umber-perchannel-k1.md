# Design brief: PerChannel K1 scalar-semantic blend restriction (2026-11-23)

Planned kill lock: `razul-perchannel-k1`.
Base: L7 status tip `f8cde643925b74b5be7ec6d19bb181b2f368d70e`.
Staged design: `docs/research/perchannel-mode-restriction-2026-10-22.md`, Option 4.

## What exists

`channel_blend_options` in `crates/umber-app/src/layers_panel.rs:91-132` offers 31 modes on `normal` and 28 on every other channel. It excludes only `Passthrough` and, off `normal`, the normal-map trio. The material channel rows consume this list directly at `:397`. The existing pure test at `:2068` pins those counts.

`layer_blend_options` in `crates/umber-app/src/document.rs:2047-2053` calls `channel_blend_options("")` for the separate layer-level compositing picker and inserts `Passthrough` only for folders. The empty string is therefore a compatibility boundary, not a material channel name. Saved documents and the driver remain permissive by design: changing the picker must not alter evaluation or rewrite stored modes.

The design's gray-content algebra proves Tint, Saturation, and Color are no-ops on conventional scalar-channel content. Value reduces to approximately Normal on gray, which is redundant but meaningful and remains allowed.

## Target

Add a small semantic predicate beside the picker and one filter clause:

- On scalar-semantic channels, hide exactly `Tint`, `Saturation`, and `Color`. Keep `Value` everywhere.
- Preserve `normal` exactly: 31 modes, including the HSV quartet and normal-map trio.
- Preserve the color-semantic set `baseColor`, `diffuse`, `albedo`, and `emissive`: 28 modes with the HSV quartet.
- Treat other unknown non-empty names as scalar-semantic: 25 modes with only the no-op trio removed. Masks, weights, roughness, metallic, height, and custom data channels take this path.
- Special-case the empty string as the layer-compositing context: preserve its existing 28-mode list and the HSV quartet. This is an explicit compatibility decision, not an exception to the unknown-channel default.
- Keep `Passthrough` and normal-map gating, enum declaration order, public picker signatures, and saved-document/driver behavior unchanged.

Add no strings and no i18n keys. Do not infer scalar semantics from plane byte shape, which the design rejects because planes are RGB-native and `MaterialOutput` has no type metadata.

## Concurrency and safety

Locked surface: `crates/umber-app/src/layers_panel.rs`, this brief, `docs/loop/kill-reports/2026-11-23-umber-perchannel-k1.md`, `docs/research/perchannel-mode-restriction-2026-10-22.md`, `docs/specs/requirements.md`, `CHANGELOG.md`, `docs/loop/queue.md`, `STATE.md`, `docs/ROADMAP.md`, `docs/loop/next-steps-2026-11-23.md`, `docs/loop/waves/2026-11-09-wave-plan.md`, and `.dragon/COORDINATION-next-wave.md`.

No driver, model, schema, persistence, dependency, release, public-PR, or second-kill scope. Any foreign dirty file, live sibling lock, origin movement, disk I/O error, or unexpected behavior delta stops the lane for re-census.

## Verification

- RED first: extend the existing picker matrix before production changes. Require roughness/scalar 25, baseColor and the empty layer context 28, and normal 31. Assert the no-op trio is absent on scalar channels, present on color channels and normal, and Value is present everywhere. Run the picker tests on the clean base and capture the failure.
- Implement the smallest predicate/filter change, then rerun the same tests green. Include `diffuse`, `albedo`, `emissive`, `height`, `metallic`, `mask`, `weight`, and an arbitrary custom name in the matrix so the color set and scalar default cannot drift silently.
- Run `~/.cargo/bin/rustfmt --edition 2021 --check crates/umber-app/src/layers_panel.rs`, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Run strict `cargo clippy -p umber-app --all-targets -- -D warnings` and full `cargo test -p umber-app` separately under `/tmp/umber-cargo.lock` with `CARGO_BUILD_JOBS=3`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_PROFILE_DEV_DEBUG=0`, and `nice -n 19`. Record every suite's passed/failed count; 0-test success is not a pass.
- Obtain an independent read-only review. Ask specifically about the empty layer context, normal's unchanged 31 modes, the color set, unknown-name scalar default, Value retention, and driver/saved-document compatibility.
- Record the kill and update the current remaining-work order, commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.

## Non-goals

No driver-side restriction, saved-param rewriting, Value removal, full Painter mode tables, MaterialOutput type threading, tooltip/K2 work, new i18n strings, export warnings for colored scalar planes, app UI restructuring, release action, or second kill.
