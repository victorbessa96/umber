# Kill report: late UI/UX findings closed (2026-11-28)

- Lock: `razul-uiux-late-review` plus `razul-uiux-keyboard`.
- Base: `966b2be0`.
- Source: `deleg_320688ac-4` against `ec979743`, reconciled to current code rather than accepted as a fresh diff.

## Landed fixes

1. **Dock floors.** The too-small split now clamps to 120 pt when both leaves can fit. Below 240 pt it preserves a valid fraction rather than manufacturing a negative second leaf. Narrow boot and custom saved layouts first tab 2D with 3D, then fold the right column into its icon rail. Compaction runs only on width changes; a same-width manual unfold is not reversed, and Reset Layout clears the hysteresis.
2. **Rail glyphs.** Bakes uses `file` and Graph uses `panel-left`, distinct from Toggle Grid and Cycle View Mode. The binary census covers every fold-rail icon against every top-strip action.
3. **No-mesh actions.** A shared `empty_state::no_mesh_action` shows the explanation and a real Open Mesh button. Bakes and Export drain one-shot requests into the existing `home_actions` channel.
4. **Localization.** Quick Setup reuses the shared native language mapper. Vector dash hints use `vector.stroke-dash-solid`; the parser accepts English and the active localized alias case-insensitively. A new flattened single-word guard permits only technical tokens SVG/PNG/body/in. The previous bake `Image files` literal was already absent.

## Already closed at reconciliation

- Curve points already have named controls, arrow-key movement, deletion, and keyboard regressions.
- Graph output uses `graphic_fg`; curve handles use `graphic_fg` with a focus ring, not hard-coded white.
- Reorder arrows, material refresh, post-effect removal, and saved-search deletion already use localized `glyph_button` labels.
- AssetStore GC and the complete icon atlas are accurately described in requirements and parity.

## ROADMAP corrections

- The OccluderScope combo→request→resolution kittest is now recorded as landed, not residue.
- The SVG archive records K6c through K8i rather than stopping at K6b.
- The UI/UX row names the actual remaining work instead of claiming only U5: U5 migration and desktop checks 1–4 remain open, while the F12/F13/F15/F17 code fixes and R3-1/R3-5 accessibility closures landed here. F16 stays a recorded deferral.

## Review history

- Round 1 `opencode/beam-free`: FIX-FIRST for tiny-window validity, doc attachment, exact floor oracle, fold hysteresis, and localized-alias casing.
- Round 2: FIX-FIRST for Reset Layout not clearing `last_compact_width`.
- Round 3: **SHIP**, zero findings.

## Verification

- Focused RED/GREEN: rail collisions, localized parser and guard, no-mesh actions, absolute floor, custom saved layouts, tiny-window validity, and width hysteresis.
- Full final app suite: **939 passed, 0 failed**; every integration target green.
- `cargo clippy -p umber-app -p umber-gui -p umber-export -p umber-cli --all-targets -- -D warnings`: clean.
- Rustfmt, `git diff --check`, scorecard (**3,360**), layers, and parity: clean.
- Targeted Windows CI is recorded separately after dispatch; no unfiltered full-matrix claim is made here.
