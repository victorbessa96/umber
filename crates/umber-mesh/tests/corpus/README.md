# Hostile-input corpus (queue row 156, sub-item A)

Every file under this directory is hostile input. The always-on suite
`../hostile_corpus.rs` runs **every file through every importer entry
point** (`load`, `gltf::load_gltf`, `fbx::load_fbx`, `usd::load_usda`,
`usdc::load_usdc`, `usdc::load_usdz`) and requires a typed `Err` from
each — never a panic, never a silent `Ok`.

Layout, one subdir per format:

- `usda/` — truncated/missing header, declared counts > actual indices,
  zero/negative/huge counts, NaN and Inf points, out-of-range and
  negative indices, truncation mid-array, nesting-depth bomb,
  unterminated string, non-UTF-8 bytes.
- `gltf/` — truncated/bad/lying GLB headers, accessor count 0 and
  count > 2^28, out-of-range indices, NaN positions, bufferView past
  the buffer's EOF, plus the kill-2 fuzz-battery mutants:
  `oob_position_index_iter*` (POSITION attribute pointing past the
  `accessors` array — gltf-json's validation hook indexes it unchecked)
  and `glb_len_underflow_iter*` (GLB header declaring total length 0 —
  gltf's `header.length - 12` underflows).
- `fbx/` — truncated magic, garbage bytes, truncated ASCII FBX.
- `usdc/` — bad magic, truncated magic, magic + garbage.
- `usdz/` — decompression bombs (stored/deflate/ZIP64 entries declaring
  4–10 GiB uncompressed from ~130-byte packages; rejected by the
  central-directory preflight in `src/usdc.rs` before any inflate),
  truncated zip, non-zip, and a well-formed zip whose root layer is
  not USD.

The files are generated, not hand-edited: regenerate with

    python3 crates/umber-mesh/tests/corpus/gen.py

Any fuzz finding (`../hostile_fuzz.rs`, battery lane) that panics or is
wrongly accepted becomes a new file here, so the always-on suite covers
it permanently.
