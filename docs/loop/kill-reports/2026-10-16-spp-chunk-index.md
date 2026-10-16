# Kill report: every chunk of a chunked .spp dataset now decodes (the import audit's fix)

Date: 2026-10-16. Author: dragon. Lock: none (a solo fix, main tree).
Trigger: the SPP-parity exploration pass (2026-10-16) asked for an
import-completeness validation; the audit found the importer silently
dropping ~95% of every real file's pixels.

## Finding (measured, not inferred)

The `texture-cache/` decode path was correct — `decode()` on the full
raw payload of a real corpus channel returns the full 4096² RGBA,
100% nonzero, matching the reference decoder (h5py + python-lz4 on the
same bytes: full 64 MiB decode, coherent image, mean (82,29,41) on the
Body base colour — the §7a measurement, reproduced exactly).

The HDF5 layer was not. `decode_chunked` walked the chunk grid with an
odometer that counts chunk INDICES (0, 1, 2, …) but looked those
indices up in the b-tree index, whose keys are the chunks' start
offsets IN ELEMENTS (chunk 3 of a 65536-element grid keys 196608 —
pinned on the corpus via h5py's `get_chunk_info`). Only chunk 0
satisfied `0 == 0`; every later chunk missed the map and decoded as
the fill value: zeros.

Consequences on the real corpus (measured before the fix):

- Every texture-cache channel imported exactly its FIRST CHUNK and
  zero-filled the rest: 1.5-6% nonzero texels on 4096² channels
  (Body baseColor: 1.5%; the reference: 100%).
- `paint/scene.assbin` (52 chunks) decoded only its first 64 KiB, so
  `material_names` found zero pairs and every set kept the
  `cache-<id>` fallback name (extract wrote `cache-…_baseColor.png`
  instead of `Body_baseColor.png`).
- The 2026-10-15 corpus hunt's "truncated lz4 blocks end in an explicit
  offset-0 marker" reading was this bug's own zero-fill misread as
  Painter padding: the [real chunk][zeros…] stream walks into the
  zero region, reads token 0x00 + offset 0x0000, and the prefix
  decoder stops there. No real corpus channel carries the marker
  after the fix (0 "ends early" lines in the full extract of all
  three files).

Why the gates stayed green: the corpus test asserted pixel-buffer
LENGTH (zero-fill satisfies it), not content; no name assertion existed;
the synthetic builder tests wrote chunk-index keys, sharing the
reader's convention error — `filter_mask_skip_honored` passed because
its grid's chunk dims made index == offset, and the 4×4 test's
`vec![cr, cc]` keys matched the buggy lookup symmetrically.

## Fix

- `hdf5.rs::decode_chunked`: translate the odometer's chunk index to
  the element-offset key (`coords[axis] * chunk_dims[axis]`,
  checked_mul) before the index lookup. One lookup, one line of math,
  comment cites the measurement.
- `lib.rs` builder tests corrected to write true element-offset keys
  (`vec![cr * 2, cc * 2]` for the [2,2] grid over [4,4]).
- New regression `every_chunk_of_a_multichunk_dataset_decodes`: a 1-D
  6-element dataset in three 2-element chunks (keys 0/2/4) asserts the
  assembled bytes are all three chunks — the old code zero-fills
  chunks 1 and 2 and fails it.
- `tests/corpus.rs` hardened (the audit's second deliverable):
  - no set may keep the `cache-<id>` fallback (material names must
    resolve from the scene dump);
  - every channel's nonzero-texel floor: `nonzero * 10 > texels`
    (measured margins — the bug landed 1-6%, real content runs 39%
    for a legitimately mostly-black emissive to 100%);
  - no channel may carry an "ends early" truncation note.

## Verification (host battery GREEN)

- `cargo test -p umber-spp`: 35 synthetic + 2 corpus, all green
  (17.4 s against the 1.3 GB corpus).
- `wave-battery.sh umber-spp umber-cli umber-app`: layers, fmt,
  clippy -D warnings, 37 + 74 + 652 tests — GREEN.
- `extract-spp` on BessaCreatureV2: sets named Body / Eyes / Fur
  (Painter's own material names), 19 PNGs, 0 truncation notes.
- `Body_baseColor.png` re-read from disk: 4096×4096, 100% nonzero,
  mean (82,29,40) — the §7a reference (82,29,41) within one table
  quantization step; the reference decoder's full-block decode of
  the same channel agrees.
- Cross-check: `lz4_flex::decompress_into` on the real block decodes
  the full 64 MiB with no error and no marker — the block was never
  truncated; the payload handed to it was.

## Standing lesson (recorded in the audit doc too)

A length-satisfying decode is not a complete decode. Importers over
compressed chunked containers need a CONTENT oracle: the corpus gate
now pins nonzero floors and reference-agreement, not just shapes.
