# Design brief: umber-vector-functional-rgb (K8i, 2026-11-23)

Planned kill lock: `razul-vector-functional-rgb`.
Base: K8h status tip `9d9b9e9ab7917446245da57d9e28f54d3d56bf71`, with implementation `6e2e5b40a45ec3f97d5381bdb8759aa1c5aade34`.

## What exists

`crates/umber-core/src/svg.rs:2461` (`parse_svg_color`) accepts `none`, `#RGB`, and `#RRGGBB`; every other fill/stroke paint form is a typed `SvgError::Unsupported`. The parser feeds editable-vector root/group/leaf inheritance, inline-style declarations, hidden-leaf validation, opacity composition, and the public `import_svg`/`import_svg_many` APIs. `crates/umber-core/src/vector_svg.rs` exports canonical uppercase six-digit hex and already owns the import/export tests.

The raster stencil treats non-`none` paint as coverage and does not parse color values. No model, serialization, app, or export-production change is needed for this import-only syntax addition.

Normative source: SVG 1.1 (Second Edition) §12.1 requires the CSS2 color-unit behavior; CSS2 §4.3.6 defines `rgb(` followed by three comma-separated integer or percentage values and `)`. Whitespace is allowed around the numerical values. Integer 255 and 100% both map to `#FF`; out-of-sRGB values are clipped (CSS2 examples include 300→255, -10→0, and 110%→100%).

## Target

K8i extends only the existing editable-vector `parse_svg_color` entry point:

- Accept ASCII-case-insensitive `rgb(...)` with exactly three comma-separated components, all integers or all percentages. Accept CSS signs and percentage decimals; reject exponent notation, mixed integer/percentage triples, missing/extra components, empty components, trailing junk, and CSS4 space-separated or slash-alpha syntax.
- Clip integer components to 0..=255 and percentage components to 0..=100 as CSS2 specifies. Convert percentages to the nearest u8 with the existing opacity conversion's round-half-up convention; document that representational choice.
- Preserve `none`, `#RGB`, and `#RRGGBB` byte-for-byte behavior and all existing typed error behavior for unsupported paint. `rgba`, named colors, `currentColor`, ICC/paint servers, and every other functional color notation remain typed refusals.
- Route presentation attributes and K8h inline-style declarations through this same function so fill/stroke, root/group inheritance, child override, opacity, and hidden-leaf validation gain identical behavior.
- Export remains canonical uppercase `#RRGGBB`; source `rgb()` spelling is not preserved. Canonical export→import→export remains byte-stable.
- Raster stencil coverage and all model/schema/app behavior remain unchanged.

## Concurrency and safety

Locked files planned: `crates/umber-core/src/svg.rs`, `crates/umber-core/src/vector_svg.rs`, this brief, `docs/loop/kill-reports/2026-11-23-umber-vector-functional-rgb.md`, `docs/loop/queue.md`, `.dragon/COORDINATION-next-wave.md`, `STATE.md`, and `docs/ROADMAP.md`.

The source/test lock is acquired before the first RED test edit. The kill uses no new dependency and touches no manifest. Any newly dirty foreign file, live sibling lock, origin movement, disk pressure, or I/O error stops the lane for re-census rather than being overwritten.

## Verification

- RED first: new public-import tests must fail on the current `rgb()` refusal before production parsing changes.
- Pin compact/spaced and mixed-case functions, all-integer and all-percentage triples, optional signs, exact 0/255 and 0%/100% endpoints, 300/-10/110% clipping, nearest-byte percentage conversion, fill/stroke opacity, root/group/leaf inheritance and override, K8h inline style, and canonical hex re-export.
- Pin malformed arity/commas/empty values, mixed forms, decimals in integer triples, exponents, non-finite spellings, Unicode lookalikes, `rgba`, CSS4 space/slash forms, and malformed hidden-leaf paint validation.
- Exercise the raster stencil on the same functional paint to prove its coverage behavior did not change.
- Run `~/.cargo/bin/rustfmt --edition 2021 --check` on touched Rust, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, strict `cargo clippy -p umber-core --all-targets -- -D warnings`, and full `cargo test -p umber-core` separately under `/tmp/umber-cargo.lock` with the slow-build environment. Record every suite's passed/failed count; 0-test success is not a pass.
- Obtain an independent bounded review, disposition every finding, and rerun affected focused/full gates before the kill report.
- Commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`.

## Non-goals

No CSS stylesheets/selectors/classes, CSS4 color syntax, named colors, `currentColor`, `rgba`/alpha functions, non-RGB color spaces, ICC/paint servers, source-spelling preservation, raster color semantics, model/schema change, app UI change, release action, public PR, version bump, or second kill.
