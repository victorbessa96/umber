# Kill report: row 156-A hostile-input sweep, kill 1 — 2026-11-05

Commit `cafe4ce` (claw-built, dragon cross-reviewed). Brief: docs/loop/briefs/2026-11-05-hostile-input-sweep.md.

## Landed
- `crates/umber-mesh/tests/corpus/`: 33 hostile samples across fbx/gltf/usdc/usd/usdz
  (truncated headers, count-mismatch, huge counts, NaN/Inf, OOB indices, offsets past EOF,
  zip bomb headers declaring 4 GiB stored / 10 GiB zip64, not-a-zip, truncated zip),
  with `gen.py` (regenerates deterministically) and README class list.
- `tests/hostile_corpus.rs`: every corpus file x every pub importer entry must return a
  typed `Err`; `catch_unwind` turns any panic into a named failure. 2 tests, green.
- Panic-site conversion in fbx.rs/gltf.rs/usd.rs/usdc.rs (unwrap/expect/panic/unchecked
  indexing on input data -> crate Error). Valid-file behavior unchanged (115 lib tests ok).
- `MAX_USDZ_UNCOMPRESSED_BYTES = 1 GiB` cap checked against the zip central directory
  BEFORE openusd extracts — bombs refused pre-inflate.
- `tests/hostile_fuzz.rs`: `#[ignore]`d seeded mutational lane (byte flips/inserts/truncates
  over corpus seeds, `UMBER_FUZZ_ITERS` default 100k), catch_unwind per case.

## Battery
fmt clean; `clippy -p umber-mesh --all-targets -D warnings` clean; `cargo test -p umber-mesh`:
115 lib + 2 hostile_corpus + 32 panic_hunt passed, 0 failed, 1 ignored (fuzz lane).

## Owed next
`cargo test -p umber-mesh -- --ignored hostile_fuzz` at 200k iters in a battery session;
any finding becomes a corpus file + fix in the same kill.
