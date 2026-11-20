# Design brief: umber-vector-svg-import-hardening (K5b review closeout, 2026-11-19)

Kill lock: `razul-vector-svg-import-hardening`. Base: K5b code `0d1b6ad7`,
landing record `f925bae4`, pushed truth `edcfd035`. This kill dispositions the
late-arriving full Kimi K3.1 Pro review of K5b. The earlier inline delta SHIP
covered only the root-transform fix; it did not cover these findings.

## Findings and binding dispositions

1. **HIGH, unconsumed presentation attrs.** Root `fill-rule="evenodd"`, root
   paint/style, and path global `opacity` are currently accepted then ignored.
   Keep root transform support, but reject root paint/style/display/visibility/
   opacity attributes with typed `SvgError::Unsupported`; allow an explicit root
   `fill-rule="nonzero"` only because it matches the implemented rule. Reject
   non-nonzero root rules and path `opacity`, `display`, and `visibility` rather
   than silently importing the wrong visibility or paint. Add exact refusals.
2. **MEDIUM, roundtrip matrix.** Make the canonical K5a→K5b→K5a test
   table-driven over closed line+cubic, open line, open cubic, and stroke-alpha
   shapes. Assert exact `VectorShape` equality and byte-identical re-export for
   each row. Open cases use transparent fill because K5a deliberately exports
   open fill as `none`.
3. **MEDIUM, real I/O and failure atomicity.** Inject metadata/read closures
   into the capped SVG reader. Prove over-cap metadata refuses before the read
   closure can run. Factor import-after-read so tests can drive read and parse
   failures while asserting layers, vector params, dirty flag, raw history, and
   session history are unchanged. Success still returns one new layer through
   one grouped AddVector undo row. Shell selection changes only on Imported.
4. **LOW, exact Q/T assertions.** Replace tolerance checks with exact f32
   coordinates derived from the specified f64 quadratic elevation and smooth
   reflection formulas.
5. **LOW, empty-state failure notice.** Return the stored panel outcome when no
   vector layer exists, so a failed import in the empty state is not swallowed.
6. **LOW, search vocabulary.** Add `vector.import-svg` to `ROW_LABEL_KEYS` and
   the expected labels in the same position as the drawn button.
7. **LOW, stale module docs.** Update `vector_svg.rs` module docs to describe
   both K5a export and K5b bounded import delegation to `umber_core::svg`.

## Tests and gates

- Focused core import suite covers every new refusal variant, the four-row
  roundtrip matrix, exact Q/T coordinates, and all prior import/raster behavior.
- Focused app tests cover metadata-before-read with a must-not-run read closure,
  read failure, parse failure, unchanged document/history/dirty state, grouped
  success, empty-state outcome, and search labels.
- rustfmt, check-layers, scorecard, `git diff --check`, five-crate clippy and
  tests under the shared flock. Bounded Kimi K3.1 Pro read-only delta review
  before commit. Kill report, queue, STATE, ROADMAP, coordination, task-progress,
  and additive vault checkpoint at landing. Path-limited dcommit, safe-push with
  both owned session ids, origin read-back, then lock release.

## Non-goals

No CSS inheritance implementation, group/multi-path import, arc fitting, paint
servers, per-path transforms, native-dialog automation, version bump, release
tag, or public PR.
