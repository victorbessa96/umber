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

## A second bug found in review: payloads over one window

The original loop called `decompress(.., FlushDecompress::Finish)` once
per 64 KiB window. The backend is miniz_oxide or zlib-rs, depending on
which flate2 feature wins: `zip` 8.6 enables `flate2/zlib-rs`, so the
workspace build is zlib-rs, while `umber-spp`'s own test build may be
miniz_oxide. Both refuse that pattern, read from the registry source:
- **miniz_oxide 0.9.1** `inflate/stream.rs:227`: `Finish` on the first
  call decodes in one shot into a non-wrapping buffer. Too small →
  `Failed` → `MZError::Buf`. flate2 maps that to `Status::BufError`, which
  the loop called "stalled".
- **zlib-rs 0.6.8** `inflate.rs:2450`: with `Finish`, a call that does
  not end the stream returns `BufError`. Same result.

So every DEFLATED dataset or chunk over 64 KiB failed to open. The
synthetic tests only use 2×2 f32 chunks (64 bytes), so nothing covered
it. The rewrite:
- streams with `FlushDecompress::None`, which resumes across windows on
  both backends, until `StreamEnd`;
- continues on `Ok` or `BufError` while input or output moved;
- refuses "ends early (truncated chunk)" when neither moved.

**This is inferred from the backends' source, not observed.** The new
test (300 KB patterned payload, five windows) fails on the old loop if
the reading is right; the host confirms it.

**Real-file reach: none, measured.** The corpus is NOT empty: three of
Bessa's own Painter projects sit in `~/corpora/umber/spp/` (85 MB,
437 MB, 777 MB; queue #16's "blocked" is stale, only `manifest.json` is
missing). h5py 3.16.0, from a scratch venv, opens all three:
- 330 datasets, 196 chunked, every chunk 65,536 elements;
- **zero use any filter** (no deflate, shuffle or fletcher32).

Painter stores raw bytes in 64 KiB chunks. Neither this bug nor the bomb
can touch a real Painter file. Both live only on hand-made or other
writers' `.spp`/HDF5. The bound still matters for a hostile file; the
window fix keeps the deflate path correct for whatever does use it.

## Test

`inflate_tests::a_deflate_bomb_stops_at_its_footprint`:
- 64 MiB of zeros (deflated under 128 KB) at a 4 KiB footprint is
  refused with the footprint message;
- an honest 4 KiB stream decodes exactly;
- a short stream decodes, and the caller's check owns that refusal;
- a 300 KB patterned stream decodes byte-exact across five windows;
- the same stream cut in half is refused as truncated, not looped on.

Mutation check due on the host: with `limit` ignored (`room = 65536`),
the bomb case must fail.

## The sweep's other decoders (no change needed)

Each untrusted reader was checked for output sized by the file rather
than by a cap:

- **PNG** (`umber_export::png::read_png_rgba8`, used for paint tiles,
  mesh-map imports and environments): `png::Decoder::new_with_limits`
  with `DECODE_LIMIT_BYTES`. Bounded.
- **EXR** (`read_exr_rgba_f32`): a `MetaData` header preflight runs
  `check_exr_size` on every layer before the prelude allocates. Bounded.
- **USDC** (openusd 0.8.0, read from the registry): the LZ4 output is
  `min(estimated, compressed × 255 + 64, 4 GiB)`, which is LZ4's own
  maximum expansion, and path tables use `try_reserve_exact`. Bounded by
  the input, so a bomb needs a file of proportional size. The 4 GiB
  ceiling is upstream's; a tighter cap would be a fork, so it is not
  worth it for a mesh importer.
- **USDZ** (openusd `usdz/reader.rs:94-108`): refuses any entry that
  is not `Stored` (uncompressed, as the USDZ spec requires) and any
  entry whose compressed size differs from its size. A zip bomb cannot
  get through.
- **PSD** (`psd_import`): `MAX_PSD_SIDE` 8192 and `MAX_PSD_BYTES`
  256 MiB, checked before decode. Bounded.
- **ICC `embed_iccp`**: an unbounded `png::Decoder::new`, but it only
  ever reads back the file Umber itself just wrote. Not an input
  surface.
