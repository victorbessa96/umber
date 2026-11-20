# Kill report: umber-vector-stroke-join (K6c, 2026-11-20)

- Kill lock: `razul-vector-stroke-join`.
- Brief: `2e3a45b2` (`docs(vector): K6c stroke join brief`).
- Implementation: `68cbf388` (`feat(vector): K6c stroke join options`).
- Prior base: K6b stroke cap options `1839de83`; prior pushed truth `e28f54e8`.

## Delivered

`StrokeJoin` adds additive Round (default), Miter, and Bevel joins. `Stroke`
carries it with serde default and cache identity. Real turns get join
polygons: Bevel uses a flat outer triangle; Miter extends outer edges to their
intersection with 4×radius fallback to bevel. Closed paths use the closure
join and never emit endpoint caps. Cap style still controls open endpoints.
The Vector panel has a join combo through one undoable `SetVector` edit and
both i18n catalogs. SVG exports/imports all three joins, rejects invalid
values as typed Unsupported errors, and roundtrips join geometry.

Direct geometry tests cover actual turn indices, bevel/miter coordinates,
closure joins, empty polygon safety, and the absence of round caps for closed
Bevel geometry. Old stroke JSON without `join` loads as Round.

## Review beam

`opencode/beam-free` read-only review:
- R1 FIX-FIRST: closed paths emitted a round endpoint cap for Miter/Bevel.
  Fixed by forcing `endpoint=false` for closed paths and letting join style
  win at actual turns.
- R2 FIX-FIRST: two latent panics and weak integration assertions. Fixed
  with empty-polygon guards, open/closed index-0 handling, exact coordinate
  assertions, and closure triangle/no-cap assertions.
- R3 SHIP: no HIGH/MEDIUM findings. A test-honesty note was addressed by
  asserting against the actual `ellipse_point_count(2.0, 2.0)` instead of a
  guessed 64.

Logs: `vector-stroke-join-review-beam.log`,
`vector-stroke-join-review-beam-r2.log`, and
`vector-stroke-join-review-beam-r3.log` under Hermes scratch.

## Verification

- Focused core vector suite: 22 passed / 0 failed.
- Focused SVG suite: 19 passed / 0 failed.
- Vector panel tests: 10 passed / 0 failed.
- Final battery log: `/home/bessa/.hermes/cache/scratch/vector-stroke-join-final-battery.log`.
- Full six-crate clippy --all-targets -D warnings: 0 errors.
- Full battery: 21 suites, 2180 passed / 0 failed / 0 ignored.
- `cargo fmt --all -- --check`, check-layers.py, scorecard.py --check,
  `git diff --check`: all clean.
- No GPU/golden claims, release tag, version bump, .sbsar execution,
  telemetry, or public PR.

## Landed vs external verification

Landed: join model, closure-safe geometry, panel controls, SVG support,
compatibility/cache tests, and full battery.
External verification still owed: windowed join selection and mixed cap/join
smoke, plus release-ci.sh before the next public tag.
