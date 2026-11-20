# Kill report: umber-vector-svg-import-hardening (K5b review closeout, 2026-11-19)

Authorship: dragon implementation and verification. Review: late full Kimi K3.1
Pro K5b review, then bounded OpenCode DeepSeek hardening reviews after Kimi
stalled twice with zero output. Kill lock: `razul-vector-svg-import-hardening`.
Base: K5b `0d1b6ad7`, pushed truth `edcfd035`, hardening brief `d6e2ba35`.
Code landing: `6ffec3bc`.

Implements `docs/loop/briefs/2026-11-19-umber-vector-svg-import-hardening.md`.
The delayed original Kimi review contained seven findings beyond the earlier
root-transform issue; the short inline Kimi SHIP had covered only that one
delta. This kill closes the full review instead of dropping it.

## Built

- Typed `SvgError::Unsupported` for unconsumed root and path presentation:
  paint/style/opacity/display/visibility, non-nonzero fill rules, dash array /
  offset, clip paths, masks, filters, markers, vector effects, and paint order.
  Explicit root `fill-rule="nonzero"` remains valid; root transform still
  composes exactly once. Path effects refuse before `build_path`, so malformed
  geometry cannot mask the intended refusal.
- Canonical roundtrip matrix over closed line+cubic, open line, open cubic, and
  stroke alpha, with exact `VectorShape` equality and byte-identical re-export.
  Separate tests pin deliberate open/zero-alpha fill normalization and every
  fill/stroke alpha byte 0..=255.
- `import_with_io` / `read_svg_capped_with` inject metadata and read closures.
  Over-cap metadata refuses before a read closure can run; read and parse
  failures leave layers, vector params, dirty flag, raw history, and session
  history unchanged. Successful import remains one grouped Add+SetVector undo
  step. Production file reads use `File::open` + `Read::take(MAX+1)`, so FIFO or
  zero-metadata paths cannot allocate without the cap. All errors carry the
  selected path once.
- Empty vector panels return their import outcome, preserving ImportFailed.
  `vector.import-svg` is searchable in draw order; a real egui_kittest clicks
  Import SVG through an injected failure and proves the empty document stays
  unchanged. The order assertion scans production draw sites only.
- `vector_svg` docs now describe both export and bounded import. Q/T tests use
  exact f32 coordinates derived from f64 formulas, not tolerances.
- Number formatting uses Rust shortest f32 Display, fixing small-value loss in
  the old fixed-nine-decimal exporter. The canonical golden uses the actual
  `0.5019608` spelling, and small values parse back bit-exactly.
- Self-closing roots seed no parent frame; the tokenizer's trailing-content
  refusal remains the outer guard.

## Review disposition

1. Original seven findings: all closed and covered by focused tests.
2. DeepSeek round 1 found additional effect keys, small-f32 precision loss,
   late effect checking, and test-honesty gaps: closed.
3. DeepSeek round 2 returned SHIP with LOW follow-ups: self-closing roots,
   capped streaming reads, `paint-order`, production-only label ordering, and
   exhaustive alpha coverage: closed.
4. Final DeepSeek review returned SHIP with no HIGH/MEDIUM defects. Its last
   test-honesty notes were closed by stripping both the const and test modules
   from the label-order scan, completing the refusal matrix, renaming the
   self-closing-root test honestly, and sweeping stroke alpha.
5. The alleged `SessionHistory::truncate_future` bug was rejected with source
   evidence: it filters `undo_depth <= cursor`; it takes undo depth, not a
   vector index. `Document::run_grouped` passes the correct raw undo depth.

Kimi stalled on both the full-diff and inline hardening reviews with empty
logs. Those processes were killed; DeepSeek performed the hardening reviews.
No claw edits, claw cargo, or claw commits were used.

## Verification

- Focused core vector SVG suite: **16 passed / 0 failed**, including four-case
  roundtrip, every fill/stroke alpha byte, small-f32 precision, expanded typed
  refusals, early effect refusal, and self-closing-root handling.
- Focused app vector panel suite: **10 passed / 0 failed**, including
  metadata-before-read, capped streaming read, read/parse failure atomicity,
  grouped success, and the empty-panel ImportFailed kittest.
- Final static gate: rustfmt clean; check-layers GREEN (16 crates); scorecard
  current; `git diff --check` clean.
- Final five-crate clippy + test battery: **clippy 0 errors; 19 suites, 1,953
  passed / 0 failed**. Receipt:
  `~/.hermes/cache/scratch/vector-svg-import-hardening-final-battery-r3.log`.
- Final review receipt:
  `~/.hermes/cache/scratch/vector-svg-import-hardening-review-final-opencode.log`.

## Unverified and deferred

- Native open-dialog behavior and third-party interoperability beyond the exact
  canonical roundtrip remain manual checks.
- Full SVG/CSS inheritance, groups, multiple paths/subpaths, arc fitting, paint
  servers, and per-path transforms remain deliberately outside the subset.
- Fill cache-lifecycle parity remains the separate named follow-up.
