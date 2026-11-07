# abr-core — result (clean-room `.abr` v6/v7 `samp` reader)

## Status

- Implemented `crates/umber-brush/src/abr.rs` — the only source file created.
- No other file was touched (no `lib.rs`, no `Cargo.toml`, no docs, no layout).
- No cargo command was run; no git command was run.
- The dragon must add `pub mod abr;` to `crates/umber-brush/src/lib.rs` and
  (if desired) re-export `read_abr`, `AbrBrush`, `AbrError`.

## Public API landed (exact)

```rust
/// Maximum number of brushes accepted from one file.
pub const MAX_BRUSHES: usize = 4096;

/// Maximum accepted stamp width and height, in pixels.
pub const MAX_STAMP_DIMENSION: u32 = 4096;

/// One parsed brush from an `.abr` file.
#[derive(Debug, Clone)]
pub struct AbrBrush {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<u8>,
    pub diameter: f32,
    pub hardness: f32,
    pub spacing: f32,
}

/// Errors `read_abr` can return.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AbrError {
    NotAbr,
    UnsupportedVersion { version: i16 },
    UnsupportedMinorVersion { minor: i16 },
    Malformed { detail: String },
    TooManyBrushes { count: usize, max: usize },
    StampTooLarge { width: u32, height: u32, max: u32 },
    UnsupportedBitDepth { depth: i16 },
    UnsupportedCompression { compression: u8 },
}

/// Parses every brush in an `.abr` file.
pub fn read_abr(bytes: &[u8]) -> Result<Vec<AbrBrush>, AbrError>;
```

`AbrError` is `Clone + PartialEq + Eq` (like `PresetError`), so the wiring can
match variants and tests can compare them. Every variant has a `#[error]`
message (user-facing English; `umber-brush` has no `tr()`).

## What it does

- Version gate: v6/v7 accepted; v1–v5 and v8+ return `UnsupportedVersion`;
  minor versions other than 1/2 return `UnsupportedMinorVersion`; a version
  below 1 (or a file shorter than 2 bytes) returns `NotAbr`.
- Walks `8BIM` + 4-char type + u32 size sections. `samp` is parsed; every
  other type (`desc`, `patt`, `phry`, unknown) is skipped by its declared
  length, then the section is 4-byte aligned.
- Per brush: u32 body length (4-aligned), 1-byte Pascal name, the 10/264-byte
  parameter block, i32 `y`/`x`/`bottom`/`right` (h = bottom-y, w = right-x),
  i16 bit depth, u8 compression, alpha. The record is bounded by its length.
- Alpha: 8-bit raw, 8-bit RLE, and 16-bit raw (high byte kept). 16-bit RLE is
  a named error.
- RLE: one big-endian u16 byte-length per row, then PackBits rows. Header
  0..=127 = literal `header+1` bytes; 129..=255 = repeat `257-header` times;
  128 = no-op (matches the reference).
- Safety caps are checked before the alpha plane is allocated: >4096 brushes
  or >4096 px on either stamp side is an error. No `unwrap`/`expect`/`panic`
  outside `#[cfg(test)]`; all reads are bounds-checked; arithmetic is checked
  or provably in range.
- Names are decoded byte-for-byte as Latin-1 (never a UTF-8 failure).

## Pins written (7 required + 11 extra; 18 total)

Required:

1. `reads_a_synthetic_v6_brush_byte_for_byte` — a byte-built v6.1 file yields
   exactly one brush: name `"Test Tip"`, 4x2, alpha `[10,20,30,40,50,60,70,80]`
   byte-for-byte, diameter 12.0, hardness 0.5, spacing 0.25.
2. `packbits_literal_plus_repeat_matches_hand_computed_bytes` — one 6-pixel row
   encoded `[2,1,2,3,0xFE,9]` decodes to exactly `[1,2,3,9,9,9]` (literal run
   plus a 3-byte repeat; pins that 0xFE means 3, not 4).
3. `percent_parameters_become_fractions` — hardness 50 % and spacing 50 % map
   to 0.5 / 0.5; diameter 9 maps to 9.0.
4. `wrong_magic_is_not_an_abr` — empty input, version `-1`, and a v6 header
   with an `8BAD` section signature all return `AbrError::NotAbr` (no panic).
5. `truncated_alpha_data_is_malformed` — the synthetic file cut inside the
   alpha payload returns `AbrError::Malformed` (no panic, no partial success).
6. `oversized_stamp_is_rejected_before_allocating` — declared 100_000x100_000
   (10 GB) returns `StampTooLarge`; the cap check precedes the `vec!`, so the
   test can only finish because no plane is sized (that completion is the
   assertion).
7. `descriptor_only_v8_is_unsupported` — a version-8 file returns
   `UnsupportedVersion { version: 8 }`.

Extra:

8. `older_versions_are_unsupported` — v1/v2/v5 each return the named version
   error.
9. `unknown_minor_version_is_unsupported` — v6 minor 3 is named.
10. `raw_alpha_is_copied_verbatim` — 3x2 raw plane copied exactly.
11. `sixteen_bit_raw_keeps_the_high_byte` — big-endian `12 34 AB CD` → `12 AB`.
12. `packbits_128_is_a_noop` — header 128 consumes only itself.
13. `descriptor_section_is_skipped` — a `desc` section before `samp` does not
    break the read (and its 2 padding bytes are skipped).
14. `more_than_max_brushes_is_rejected` — 4097 records → `TooManyBrushes`
    with `count: 4097, max: 4096`.
15. `minor_two_reads_the_parameter_prefix` — a 264-byte v6.2 block with the
    same 10-byte prefix yields diameter 18, hardness 0.4, spacing 0.3.
16. `percentages_above_one_hundred_saturate` — 250 % clamps to 1.0.
17. `version_seven_is_accepted` — v7 shares the v6 layout.
18. `zero_length_brush_record_is_malformed` — a 0-length record errors instead
    of looping forever.

## Assumptions the dragon MUST verify against real files

1. **Big-endian.** The vendored reference uses `from_be_bytes` for every
   multi-byte read (`read_int16`, `read_uint16`, `read_int32`, `read_uint32`
   in `reader.rs`), and the ABR reader calls them unchanged, so this parser is
   big-endian throughout. The brief did not state endianness; this is the
   single highest-risk assumption.
2. **Parameter-block location.** The brief says the brush parameters come
   *after* the stamp, but the vendored reference has no post-alpha parameter
   parsing: after the alpha it jumps to `brush_end`. The only parameter-shaped
   block in the reference is the one it skips between the name and the bounds
   (10 bytes at minor 1, 264 at minor 2). This parser reads the first 10 bytes
   of that block as five BE u16s in the classic Photoshop tip-shape order:
   roundness, angle, hardness %, spacing %, diameter px. Verify on a real
   v6.1 and v6.2 file; if the v6.2 block starts differently, only
   `diameter`/`hardness`/`spacing` are wrong, never geometry or alpha.
3. **v6.2 block prefix.** The first 10 of the 264 bytes are assumed to mirror
   v6.1; the trailing 254 bytes are ignored. (If a real v6.2 file writes zeros
   there, the three parameters will be 0; the stamp still parses.)
4. **Length strictness.** The section size is treated as the exact payload
   byte count and each brush length as the body byte count, both 4-aligned on
   disk. The reference is slightly more lenient (it lets a brush body run past
   the section end). A file whose final brush padding is excluded from the
   section size would be rejected here as `Malformed`. Verify with a fixture.
5. **PackBits 128.** Treated as a no-op, per the reference (the brief's
   "repeat three times" note does not match it).
6. **Caps only.** Per the brief: 4096 brushes and 4096x4096 px per stamp.
   There is no total-decoded-bytes cap, so a highly compressible hostile file
   can still ask for up to 4096 x 16 MiB = 64 GiB of planes. Named follow-up
   if the dragon wants a total cap.
7. **16-bit raw** keeps the high byte; 16-bit RLE is rejected.

## Deliberately left out (recorded in the module docs)

- `desc`/descriptor parsing (brush dynamics, texture, dual brush) — separate
  kill; the section is skipped by length.
- `patt` pattern decoding; `phry`.
- v1–v5 and v8+; 16-bit RLE; writing `.abr` (never).
- i18n keys: the crate has no `tr()`; error strings are English and should be
  translated by `umber-app` at the call site.
- The `docs/specs/requirements.md` row mark: the brief forbids doc edits, so
  the dragon owns that cell.

## Gates run (transparent, no cargo)

- `rustfmt --check --edition 2021 crates/umber-brush/src/abr.rs` → **clean,
  exit 0** (read-only; no rewrite).
- Scratch type-check/test in `~/.hermes/cache/scratch/opencode/`: copied the
  file, removed the `thiserror` derive/attributes with a stub (the crate is
  not buildable without cargo), then `rustc --edition 2021 --test` →
  **0 errors, 0 warnings; 18/18 tests pass**. The `thiserror`-generated
  `Display`/`Error` impls themselves were not type-checked; every
  `#[error]` field name was reviewed against its variant by hand.
- **Not run:** `cargo check/test/clippy`, workspace battery, layers check.
  The dragon owns those.
