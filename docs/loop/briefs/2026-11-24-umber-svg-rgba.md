# Design brief: bounded SVG `rgba()` paint support (2026-11-24)

Planned kill lock: `razul-svg-rgba`.
Base: XI2 hotplug status tip `96d95745444c266179da776201938cd33a0b2ac0`.

## What exists

`parse_svg_color` supports `#RGB`, `#RRGGBB`, and CSS2 `rgb()` with integer or percentage components. `parse_vector_paint` composes `fill-opacity` and `stroke-opacity` through `with_alpha`. `rgba()` is explicitly refused as an unsupported color function. Export already canonicalizes to uppercase hex plus the appropriate opacity attribute, so it does not need a new output form.

The remaining slice is one measured color-function gap, not the whole CSS backlog: `rgba()` with alpha composition.

## Target

- Parse CSS2 `rgba(r,g,b,a)` case-insensitively, with exactly four comma-separated components.
- The first three components reuse the existing `rgb()` rules: all integer or all percentage, nearest-byte percentage conversion, gamut clipping, and no mixed forms or exponent syntax.
- The fourth component accepts either a finite number in `0..1` or a percentage in `0%..100%`, clipped to the valid alpha range and converted through the existing `with_alpha` rounding path.
- Compose alpha multiplicatively with `fill-opacity` and `stroke-opacity`: `rgba()` alpha × the presentation attribute. Default attribute opacity is 1.0, so the function's own alpha is preserved when no attribute is present.
- Return the composed paint as `[u8;4]`; the existing stroke and fill behavior, visibility, and export paths remain unchanged.
- Keep export canonical: uppercase hex plus `fill-opacity` or `stroke-opacity`, not `rgba()` output. No schema or model change.

## Failure handling

Reject missing/extra components, empty components, mixed integer/percentage RGB forms, non-finite alpha, malformed alpha percentages, non-finite RGB components, trailing junk, and CSS4 space/slash syntax with the existing `SvgError::Unsupported` paint error. `rgba()` with zero alpha behaves like any other fully transparent paint: no stroke and no painted fill effect. No silent fallback to opaque paint.

## Tests

- `rgba(255,0,128,0.5)` gives `[255,0,128,128]`; `fill-opacity=0.5` composes to 64.
- `rgba(0%,50%,100%,25%)` gives `[0,128,255,64]` with default attribute opacity.
- Stroke `rgba(10,20,30,50%)` with `stroke-opacity=0.5` gives `[10,20,30,64]`.
- Case-insensitive `RGBA(...)` works; zero alpha is transparent.
- Canonical export→import→export remains byte-identical and emits hex + opacity, not `rgba()`.
- The malformed cases above are refused with the existing paint error, never accepted as opaque.
- RED first: prove `rgba()` is currently unsupported, then implement the parser and alpha composition.

## Concurrency and safety

Locked surface: `crates/umber-core/src/svg.rs`, `crates/umber-core/src/vector_svg.rs`, this brief/report, `docs/ROADMAP.md`, `docs/loop/waves/2026-11-09-wave-plan.md`, and the current status/queue surfaces. No sibling lock or other source surface is touched. Any foreign dirty file, live sibling lock, origin movement, disk I/O error, or unexpected behavior delta stops the lane for re-census.

## Verification and acceptance

- RED first: run exact `rgba()` acceptance tests and capture the unsupported-paint failure.
- Run strict `cargo clippy -p umber-core --all-targets -- -D warnings` and full `cargo test -p umber-core` separately under `/tmp/umber-cargo.lock` with the slow-build environment. Record passed/failed counts; zero-test success is not a pass.
- Run rustfmt, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Obtain independent review of component validation, alpha composition, canonical export, and failure behavior.
- Record the kill and update the current remaining-work order, commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`.

## Non-goals

No CSS4 space/slash syntax, named colors, `currentColor`, stylesheets/selectors/classes, paint servers, root/group `vector-effect`, source-style preservation, or second-kill scope. The control-channel smoke tail remains later independent work.
