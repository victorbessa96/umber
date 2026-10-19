# stencil-soft-edges — Amendment B's rasterize-only change (the design's own follow-up closed)

2026-10-19. Coded by the opencode claw on
`opencode/deepseek-v4.1-flash-free` against this session's pinned
design (Bessa's steer); cross-reviewed, censused, battery-verified
and committed by session local-3d2151f3. Lock:
stencil-soft-edges (umber-core/stencil.rs, the app's doc lines,
the design doc's Q3 disposition, this report). The full claw
report: /tmp/claw-soft-edges-report.md (preserved in substance
below; the originals live in the session log).

## The change

`rasterize_gate` writes the bilinear sample ROUNDED to the nearest
byte (0..=255) — no threshold. `GATE_THRESHOLD` removed (its only
consumer was the replaced step; leaving it dead would fail clippy
-D warnings). The LOAD-time alpha cut (ALPHA_CUT = 128) is
UNCHANGED by contract — binary images stay binary in the loader;
the softness comes from sampling. Interior texels of a binary
image sample their own texel exactly (weight 1), so every existing
binary expectation holds; only boundary-straddling samples change.

## The tests (the claw's hand-derived work)

- `half_texel_boundary_samples_read_the_soft_coverage` (reworked
  from the old inclusive-threshold pin): the full ramp over a 2x1
  binary edge — 64 / 128 / 191 at quarter/half/three-quarter
  weights, with exact 255/0 endpoints and the arithmetic in the
  comment.
- `two_x_scale_doubles_each_texel_and_softens_the_ring` (reworked):
  the full 10x10 soft matrix, every entry hand-derived (worked
  examples in the test: 16, 48, 207, 239...), replacing the old
  binary ring.
- The identity/out-of-coverage/degenerate/loader tests untouched
  — the regression proof that soft is a boundary effect only.
- The claw re-implemented the core rasterize in Python (the
  embedded port in the stencil-core kill report is markdown,
  outside its whitelist) and re-derived EVERY pinned expectation
  baseline-match, PLUS the mutation rung: re-introducing a
  threshold (either > or >= 127.5) is killed by the new [128]
  boundary expectation. Named follow-up (dragon): append the soft
  expectations to the stencil-core report's port.

## Doc truth (three sites, the claw found one beyond the brief)

- umber-core module docs + rasterize_gate's doc: soft at
  boundaries since 2026-10-19; the loader cut explicitly named.
- umber-app stencil.rs: BOTH binary-gate claims updated (the
  brief's :24 line AND the second one at :79 — "the gate stays
  binary whatever this is" would have shipped known-false).
- The design doc's Closure-tail Q3 disposition: "binary v1
  shipped, then soft edges LANDED 2026-10-19 (`stencil-soft-edges`)
  — rasterize-only... both composition sites unchanged per
  Amendment B (label min, gate_plane product)". Amendment B's own
  text untouched, exactly as its contract demanded.

## Cross-review + gates

- Census GREEN 10/10 (the standing script:
  hermes-verify-stencil-soft-edges.py); rustfmt CLEAN both files.
- Host battery (wave-battery.sh umber-core umber-app):
  BATTERY_EXIT=0 — fmt PASS, clippy PASS (-D warnings),
  umber-core 126 tests (the reworked matrices green — the
  hand-derived expectations were exact) + umber-app 822.
- verify-kill PASS with one WARN (the documented matches!-arity
  false positive, pre-existing).

## Semantics note (Amendment B, pinned before this kill)

The label's compose stays `min`, gate_plane's fold stays product —
identical for binary bytes, and for soft bytes min keeps partial
coverage honest at the label while the product is in force at
paint time. The two-site divergence is the contract; this kill did
not touch either site.
