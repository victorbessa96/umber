# Kill report: sppv2-r1-reader (f16 read half)

- Authorship: dragon (solo, host-down pass).
- Kill-lock `sppv2-r1`: `crates/spp/src/lib.rs`, `hdf5.rs`,
  `texture_cache.rs`, `model.rs` (edits: hdf5.rs, model.rs,
  texture_cache.rs).
- Implements the READ half of import-audit R1 (full-precision channel
  import): the importer now RECOGNIZES and DECODES IEEE-754 binary16
  channels. The NATIVE-precision payload half (ChannelImage carrying
  f16/f32 planes for Umber's f16 plane formats) and the extract CLI's
  `--precision native|8` flag are the kill's second half — deferred
  below, host-gated for the corpus tolerance test.

## Built

- `hdf5.rs`:
  - `Dtype::F16` (byte_size 2, name "f16").
  - `parse_datatype`'s float arm accepts the binary16 property tuple
    `(size 2, exp 10/5, mant 0/10, bias 15)` — previously
    "non-IEEE float" and refused.
- `model.rs`:
  - The 8-bit sample closure gains the F16 arm: decode → the same
    [0,1] clamp as f32/f64 (NaN→0, ≥1→255, round).
  - The report note: "f16 narrowed to 8-bit (preview; native payload
    kept)".
  - Tests (3): `f16_bits_decode_to_exact_f32_values` (9 pinned
    patterns from Python `struct` ground truth: 0x3C00=1.0, 0x7BFF=
    65504.0, ±inf, 0x3555=0.33325195…), `f16_subnormals_decode_to_
    the_smallest_representable` (0x0001 = 2^-24 exactly, 0x0003 = 3·2⁻²⁴,
    sign bit), `an_f16_channel_decodes_to_the_8bit_preview` (a 1×1×4
    f16 dataset of 0x3800 decodes to 128s + the note).
- `texture_cache.rs`: the existing private `f16_to_f32` promoted
  `pub(crate)` — one converter, not two. The first draft of this kill
  wrote a duplicate converter in model.rs; the reuse rule caught it,
  the duplicate is deleted, and all call sites point at the shared
  helper.

## Judgment calls

- **Reuse over duplicate** (the texture_cache helper): its formula
  (`frac * 2^-24` for subnormals) is exact on every ground-truth
  pattern — verified by the Python sim over 12 patterns before
  trusting it. My first converter had an off-by-two bias bug in
  subnormal normalization that the Python ground truth caught
  (0x0001 → 101<<23 vs the correct 103<<23); the existing helper was
  right all along, which is the reuse rule working exactly as it
  should.
- **The v1 8-bit path unchanged in shape**: f16 samples clamp to
  [0,1] exactly like f32 — a normal map's negative hemisphere reads
  as 0 in the flattened preview (the corpus already carries this
  behavior for f32 normal channels; R1's "normals keep their float
  range" is the NATIVE payload half's contract, named in the note).
- **The f16 property tuple pinned exactly** (2, 10, 5, 0, 10, 15) —
  the HDF5 spec's binary16 layout; anything else still refuses with
  the non-IEEE error, so an exotic float format cannot silently
  misread as f16.

## Unverified

- Battery (host down since 18:2x). Static: rustfmt PASS on both
  files; the converter's ground truth re-derived in Python over 12
  patterns (exact); the Dtype blast radius swept — only
  model.rs's sample closure and hdf5.rs's three accessor matches
  reference Dtype exhaustively, all covered; builder.rs and
  texture_cache.rs carry no Dtype matches.

## Deferred (the kill's second half, host-gated)

- `ChannelImage.native: Option<NativePlane>` (f16/f32 payloads beside
  the 8-bit preview) — needs the corpus's RGBA16F normal set to pin
  the tolerance test ("matches the reference decoder within
  tolerance", audit R1's acceptance).
- `extract-spp --precision native|8`.
- The corpus gate's f16 coverage (the three corpus files' normal maps
  currently import through the f32 path? — measure on the host; the
  gate's content pins name the answer).
