# Design brief — hostile-input sweep for mesh importers (queue row 156, sub-item A), 2026-11-05

Status: DESIGN. Picked as the row-156 follow-on after row 154 w1 (Bessa's order
2026-11-05: hostile-input sweep first, then UV G-buffer, packaging, COW undo).

## Scope

The four untrusted-input parsers in `crates/umber-mesh`:

| file | lines | panic!/unwrap/expect/unreachable sites |
|---|---|---|
| `fbx.rs` | 272 | 3 |
| `gltf.rs` | 437 | 4 |
| `usdc.rs` | 749 | 23 |
| `usd.rs` | 1565 | 16 |

Plus decompression preflights wherever bytes are inflated (usdc/usd zstd/zip
paths in `usdc.rs`/`usd.rs`): cap output bytes BEFORE inflating (bomb guard),
cap record counts, reject absurd dimensions/offsets at the header stage.

## Method

1. **Corpus first**: commit `crates/umber-mesh/tests/corpus/` with hand-built
   hostile samples per format (truncated header, declared-count > actual,
   zero/negative counts, huge counts, NaN/Inf floats, self-referential indices,
   offset past EOF, compressed bomb header with small payload) + the existing
   valid seeds. Every corpus file must produce a typed `Err`, never a panic.
2. **Always-on smoke**: `tests/hostile_corpus.rs` runs the whole corpus through
   every importer entry — bounded, deterministic, runs in CI.
3. **Long fuzz lane (ignored by default)**: `tests/hostile_fuzz.rs` `#[ignore]`d
   mutational loop (seeded RNG, byte flips/inserts/truncations on corpus seeds,
   N iterations configurable via env) for battery sessions — findings land as
   new corpus files so the always-on suite covers them permanently.
4. **Fix pattern**: convert each panic site to a typed error at the parse layer;
   add `.context(...)` where useful. NO `unwrap()` left on input-derived data;
   `expect` only on invariants with a comment. Slicing/indexing on input data
   goes through checked accessors returning `Err`.

## Acceptance

- `cargo test -p umber-mesh` green incl. `hostile_corpus` (all corpus → Err).
- `cargo test -p umber-mesh -- --ignored hostile_fuzz` with e.g. 200k iters
  (battery session, nice 19, flock) finds ZERO panics; any finding becomes a
  corpus file + fix in the same kill.
- Decompression paths have explicit output-byte caps; a 10 MB zip declaring
  10 GB fails fast with a typed error.
- clippy `-D warnings`, fmt, layers gate stay green.

## Non-goals

- No new dependencies (no cargo-fuzz/libfuzzer; the seeded in-test RNG is enough
  at this scale). No changes to valid-file behavior (existing 145 mesh tests
  stay green unchanged).
- No changes to umber-app import dialogs (error display of typed errors is a
  later polish row).
