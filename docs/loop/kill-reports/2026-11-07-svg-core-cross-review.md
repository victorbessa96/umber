# Cross-review receipt — svg-core (row 266, commit 5d4fcccc)

- **Reviewer:** Razul (dragon session local-1625-rz6f2a), independent pass
  over the landed `crates/umber-core/src/svg.rs` (2110 lines) + `lib.rs`.
- **Author:** the sibling session's claude claw (opus, headless), per
  `.dragon/COORDINATION-next-wave.md`'s row-266 entry.
- **Verdict:** FIX-FIRST on one class (hostile-input budgets), otherwise
  APPROVE. Fix applied by the reviewer (the author's session was not
  available to apply it): `107f90d1`.
- **Independent gates run by the reviewer** (not the author's claims):
  `cargo clippy -p umber-core --all-targets -- -D warnings` clean and
  `cargo test -p umber-core --lib` → 213 passed / 0 failed on the landed
  tree, then the fix run: same clippy clean, 214 passed / 0 failed with
  the new pin green.

## What was verified (the checks that passed)

1. **The raster cap runs before any allocation** — `parse_document`
   (`svg.rs:1478`) reads the root tag, derives width/height, and refuses
   `width * height > max_pixels` (`:1501`) BEFORE any child element is
   parsed and before the plane is allocated. Pin: `over_cap_refuses_before_allocating`.
2. **The scanline rasterizer's edge ordering invariant holds** — the
   `break` on `e.y0 > y` (`:1617`) is only sound with edges sorted by
   `y0`; `shape_from_polys` sorts them (`svg.rs:1255`). Verified.
3. **The subsample accounting is exact** — `fill_span` marks the
   subsample columns whose centres fall in `[x0, x1)`
   (`ceil(x * s - 0.5)`, `:1578`), the mask is one row wide
   (`w * SUPERSAMPLE` bools), and `coverage_byte` maps 16 hits to
   `round(255 * hits / 16)` (`:1568`). The half-covered-edge pin expects
   128 and the quarter-row pin expects 64/191 — both green.
4. **Union semantics across shapes** — the shared mask row means a
   subsample covered by two shapes counts once, which is the documented
   "shapes union into the plane" contract (`:104`).
5. **No recursion in the parser** — the element walk is an explicit
   `Vec` stack (`:1524`), so hostile nesting cannot overflow the stack.
6. **Curve flattening is bounded** — `MAX_SEGMENTS_PER_CURVE = 1024`
   caps every curve's segment count (`:135`, pin
   `segment_counts_follow_the_flatness_bound`).
7. **The module's stated non-goals match the code** — no CSS, no
   `use`/`defs`, no text, no strokes, no opacity, and the unknown-element
   policy skips subtrees; the docs say so explicitly and the code does
   exactly that.

## Findings

### F1 — MEDIUM — no structural budgets: a shape flood bypasses the raster cap

- **Evidence:** the only budget in the module is the raster-size cap
  (`DEFAULT_SVG_RASTER_PIXELS`, `:128`). Nothing bounds the number of
  shapes or the number of flattened points a document may contribute.
  `parse_document` extends `shapes` without a count check
  (`:1542` pre-fix) and `build_path` feeds `PolyBuilder` without a
  point budget (`:1365` pre-fix).
- **Why it matters:** the raster cap bounds the OUTPUT, not the work. A
  file with a million tiny `<rect>`s fits any raster and still costs a
  million shape builds, a million edge sorts, and — worse — the
  rasterizer's per-row scan is `O(rows × shapes)` (`:1605`, `:1612`),
  i.e. a 2048-row raster over a million shapes is billions of
  comparisons. The same holds for one path with a huge number of
  commands: each curve flattens to up to 1024 points, so path data
  multiplies.
- **Fix (applied in `107f90d1`):** `pub const MAX_SHAPES: usize = 65_536`
  and `pub const MAX_POINTS_PER_SHAPE: usize = 262_144`, a new
  `SvgError::TooComplex { what, limit }` variant, the shape count
  checked as the document is walked, and the per-shape point count
  checked before the edge builder. Pin:
  `complexity_budgets_refuse_shape_and_point_floods` (a shape flood and
  a point flood, each refused with the named error).

### F2 — INFO — no cap on the input string itself

- **Evidence:** `rasterize_svg` takes `&[u8]` and converts to `&str`
  (`:239`); the only budgets are the raster cap and (now) the structural
  budgets of F1.
- **Assessment:** accepted, not fixed. The caller owns the file handle
  and its size limit (the loader wave will read a file, and a file
  larger than memory is refused by the OS); the F1 budgets bound the
  WORK per byte, which is the part the core can own. Named here so the
  loader wave (row 267's follow-up) adds a byte-size cap at the file
  read.

### F3 — INFO — `preserveAspectRatio` is not implemented (documented)

- **Evidence:** the module docs state the viewBox is stretched onto the
  raster (`:~85`).
- **Assessment:** matches the design brief (row 265's decision) and is
  documented; a stretch is visible in the output rather than silent.
  No change.

## Not done in this review

- The loader wiring (the app's stencil loader accepting `.svg`) is the
  next wave per the row-265 brief; the core API (`rasterize_svg`,
  `SvgRaster::to_rgba`, `SvgError`) is the seam it consumes.
- The `.abr` wave (rows 267-268) is a separate lane.
