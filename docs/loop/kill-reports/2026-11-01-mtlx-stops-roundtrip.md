# Kill report: mtlx `stops` round-trip — closes the gradient_map export limit

Date: 2026-11-01 ~14:3x (dragon local, loop wakeup #8). Lock: `mtlx-stops-roundtrip`.
Envelope: code only, no cargo/tests (confirmed at wakeup #7); rustfmt
(edition 2021) sanctioned and clean.

## Scope

The `gradient_map` kill report (2026-11-01-g4-gradient-map.md) named the
limit: `ParamValue::Stops` was dropped on mtlx export ("no MaterialX
equivalent"). This kill makes the mtlx carry a first-class type so a
`gradient_map` node survives `.mtlx` export/import bit-identical.

## What landed

`crates/umber-graph/src/mtlx.rs` only:

- Export: `ParamValue::Stops` emits
  `<input name="stops" type="stops" value="pos,r,g,b,a; pos,r,g,b,a; …" />`
  via a new `fmt_stops` helper. `f32::to_string` is shortest-round-trip,
  so the parse returns bit-identical stops.
- Import: `value_from_typed` gains a `"stops"` arm — `;`-separated stops,
  each 5 comma-separated floats (`parse_floats(..., 5)`); empty value =
  empty ramp (eval_ramp reads that as opaque mid gray); a malformed stop
  is a parse error, never a silent fallback (same contract as `anchor`).
- Module doc's ParamValue mapping line updated (`Stops → stops`).
- Pinned test `stops_params_round_trip_exactly`: spelling
  (`value="0,0,0,0,1; 0.5,1,0.25,0.5,1; 1,1,1,1,1"`), exact round-trip, and
  the malformed-stop parse error.

## Verification (envelope-limited, honest)

- rustfmt --edition 2021 clean (parses the whole umber-graph tree, so
  syntax is proven; no type check — no cargo this session).
- Python mirror of `fmt_stops`/the parse arm: expected spelling matches
  the pinned test string, round-trip is identity, empty value → empty
  ramp, 4-float stop → error. 4/4.
- The pinned unit test itself is UNRUN (battery at the next wave push).

## Open tails

- The plugin wire stays host-only for Stops (built-ins never run as
  guests) — unchanged, still correct.
- GPU ramp op for gradient_map still open (CPU GlobalPass in v1).
