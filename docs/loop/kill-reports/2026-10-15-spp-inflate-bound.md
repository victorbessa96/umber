# Kill report (UNRUN, host-gated): SPP deflate output is bounded by the footprint

Date: 2026-10-15. Author: dragon. Lock: `spp-inflate-bound` over
`crates/spp/src/hdf5.rs` and this report. Found in a static sweep of the
untrusted-file parsers (a `.spp` project from anyone is HDF5).

## Finding

`inflate_raw` grew its output until the deflate stream ended, stopping
only at the global 256 MiB cap; the footprint check (`bytes.len() !=
expected`) ran AFTER. A hostile chunk of about 256 KB of deflated zeros
therefore allocated and filled 256 MiB before being refused (deflate's
ratio is about 1000:1). The 256 MiB cap bounded each chunk, not the
file, though the first refusal aborts the open. `run_pipeline` already
received the expected size and ignored it (`_expected`).

Severity: a memory and CPU spike on opening a hostile `.spp`; no
corruption, and the open fails cleanly. Low, but it is the one reader
path whose cost was not set by the declared shape.

## Fix

- `inflate_raw(path, input, limit)` grows its window at most to
  `limit + 1` bytes (`min(limit, 256 MiB)`). One byte past the footprint
  proves a mismatch, so it returns `Corrupt("…inflates past its N-byte
  footprint")` without decoding further.
- `run_pipeline` passes its `expected`, which is:
  - the dataspace total for compact and contiguous layouts;
  - the FULL chunk size for chunked layouts (edge chunks decode smaller
    and are still checked against their own footprint by the caller).
- Shuffle and Fletcher32 never change the size, so the post-inflate
  length equals the footprint for every legal pipeline.
- Removed the old 256 MiB `Unsupported` branch; the limit subsumes it.

## Test

`inflate_tests::a_deflate_bomb_stops_at_its_footprint`:
- 64 MiB of zeros (deflated under 128 KB) at a 4 KiB footprint is
  refused with the footprint message;
- an honest 4 KiB stream decodes exactly;
- a short stream decodes, and the caller's check owns that refusal;
- a 300 KB stream decodes across several 64 KiB windows.

Mutation check due on the host: with `limit` ignored (`room = 65536`),
the bomb case must fail.
