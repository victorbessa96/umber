# Curve keyboard access and graphic contrast

- Lock: `razul-curve-accessibility`.
- Brief: `ba65330aae81` (`docs(gui): curve accessibility and contrast brief`).
- Outcome: shared and legacy pressure curves are keyboard-operable, and graph/handle strokes follow an all-theme contrast-safe foreground.

## Implementation

- Shared `CurveEdit` exposes `curve point N` sliders. Left/Right changes x, Up/Down changes y through `Curve::move_point`; Delete/Backspace uses protected `Curve::remove` when variable count is allowed. The canvas is click-to-insert only, while each handle owns drag, secondary-click, hover, and keyboard input, so the new accessible handles cannot starve pointer behavior.
- Legacy four-point editor exposes the same named controls. Arrows go through `clamp_control_point`; Delete uses existing protected collinearization; the spline and animation are unchanged. Focus is painted around the active handle.
- egui processes arrow focus navigation at end-pass, so both editors clear `FocusDirection` after an arrow edit. This was required: the first implementation edited correctly but ArrowRight moved focus to the next point before Delete.
- `theme::graphic_fg()` maps meaningful non-text strokes to the primary-text token. Graph output borders, pending noodles, output ports, and non-hover shared handles no longer use hard-coded gold/white.

## RED/GREEN evidence

- Before the fix, real kittests failed to reach `curve point 2` in either editor.
- The old graph gold failed the 3:1 assertion on Light panel; old white handles failed on Light raised.
- After the fix, Tab reaches the named point, horizontal and vertical arrows change the real curve, interior Delete works, and endpoint Delete remains protected in both paths.
- All four themes clear 3:1 for the chosen graphic foreground on the relevant panel/raised surfaces.

## Verification

- `cargo test -p umber-gui -- --test-threads=1`: **157 passed, 0 failed**.
- `cargo test -p umber-app -- --test-threads=1`: **927 bin tests plus every lib/integration target passed, 0 failed**.
- `cargo clippy -p umber-gui -p umber-app --all-targets -- -D warnings`: clean.
- Rustfmt clean.
- Independent review found pointer starvation and two weak oracles. Steady-state drag/right-click regressions now reproduce RED before and GREEN after; legacy Delete uses a non-identity chord oracle. The legacy canvas paints its focus ring last, after the plot and presets, with a source-order regression. Final review `deleg_e4a74720`: **SHIP**, zero findings.
- Scorecard/layer/parity gates, commit/push/current-tip CI: not yet claimed.

## Non-goals

No interpolation or spline redesign, no new product feature, no release/tag/upstream PR, no centralhost work.
