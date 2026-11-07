# KILL BRIEF — abr-core: clean-room .abr v6/v7 brush sampler parser (row 268)

You are implementing ONE kill in the umber repo (Rust workspace, pure Rust, no
new dependencies). Read the repo's AGENTS.md / CLAUDE.md first if they exist
(standing rules), then implement exactly what this brief specifies.

## Hard constraints (violating any of these fails the kill)

- Create EXACTLY ONE new file: `crates/umber-brush/src/abr.rs`.
  Do NOT edit any other file — NOT `crates/umber-brush/src/lib.rs` (the
  dragon wires the module), NOT Cargo.toml, NOT docs, NOT the repo layout.
- ZERO cargo commands (no build, no test, no fmt, no clippy). The dragon
  runs every gate after you finish. Writing code is your whole job.
- No network access, no downloads, no package installs.
- No git commands (no add/commit/checkout/stash/branch).
- Never panic in non-test code: the crate denies `clippy::unwrap_used`,
  `clippy::expect_used`, `clippy::panic` in non-test builds. Return
  `Result`/`Option` instead. (Test code may use unwrap.)
- No `unwrap()`/`expect()`/`panic!` outside `#[cfg(test)]` code.
- No `todo!()`, no `unimplemented!()` left behind — every path returns a
  named error.

## What to build

A self-contained parser for Adobe's `.abr` brush files, scoped to the
**v6 / v7 `samp` sections only** (no descriptor/`desc` parsing — that is a
separate kill).

Public surface (exact names, the dragon's wiring depends on them):

```rust
/// One parsed brush from an .abr file.
#[derive(Debug, Clone)]
pub struct AbrBrush {
    pub name: String,
    /// Stamp width/height in pixels.
    pub width: u32,
    pub height: u32,
    /// 8-bit alpha plane, row-major, `width * height` bytes.
    pub alpha: Vec<u8>,
    /// Brush diameter in pixels (from the brush header).
    pub diameter: f32,
    /// Hardness 0..=1 (from the brush header, percent/100).
    pub hardness: f32,
    /// Spacing as a fraction of the diameter (percent/100).
    pub spacing: f32,
}

/// Errors the parser can return.
#[derive(Debug, thiserror::Error)]
pub enum AbrError {
    // Bad magic / not an .abr
    // Unsupported version (v1/v2/v5, and v8+ desc-only)
    // Truncated / malformed
    // A brush count or stamp size over the safety cap
    // (name each variant; the dragon's error text is user-facing)
}

/// Parses every brush in an .abr file.
pub fn read_abr(bytes: &[u8]) -> Result<Vec<AbrBrush>, AbrError>;
```

Safety caps (hostile input): at most 4096 brushes per file; each stamp at
most 4096x4096 pixels; refuse BEFORE allocating the stamp buffer.

## Format facts (verified against the vendored reference in this repo)

Reference implementation (READ IT, port the LOGIC, write your own clean-room
Rust — do not copy its comments or its Russian text):
`docs/claw-artifacts/ag-psd/src-tree/src/abr.rs`

- Section walk: `docs/claw-artifacts/ag-psd/src-tree/src/abr.rs:930` —
  every section starts with the signature `8BIM`, then a 4-char type, then
  a u32 size, then the payload.
- The `samp` section contains a sequence of brushes; per brush
  (`abr.rs:938` onward):
  - u32 brush length, padded up to a 4-byte boundary (`brush_length & 0b11
    != 0` → add bytes).
  - Pascal string (u8 length + bytes) = the brush name.
  - Version-dependent skip: 10 bytes when the file's minor version is 1,
    264 bytes otherwise.
  - i32 y, i32 x, i32 top, i32 left → bounds: `h = top - y`, `w = left - x`.
  - i16 bit depth (8 for the case you implement).
  - u16 "rows written" / column count, then the compressed (RLE) alpha
    data.
- RLE decode: `abr.rs:608` (`read_data_rle`) — uint16 byte-lengths per
  row (one per row per plane), then per row a PackBits-style stream: a
  header byte > 128 means "repeat the next byte N = 256 - header times";
  a header byte <= 128 means "copy the next N bytes literally"; 128 means
  "repeat the next byte 3 times" per the PackBits special case (check the
  reference implementation for its exact handling and match it).

After the stamp, the brush header continues with the brush's parameters
(diameter, hardness, spacing among them). Read the reference
(`abr.rs` after the alpha section) to get the exact field order for the
version you support, and map: diameter → `diameter`, hardness (a percent)
→ `hardness / 100.0`, spacing (a percent) → `spacing / 100.0`.

## Pins (tests inside `crates/umber-brush/src/abr.rs`, `#[cfg(test)] mod tests`)

Build a synthetic .abr byte-by-byte in the test module (a small helper that
writes the 8BIM/samp structure with a known 8-bit RLE stamp) and pin:

1. `read_abr` on the synthetic v6 file returns exactly one brush with the
   written name, width/height, and an alpha plane EQUAL to the bytes you
   encoded (byte-for-byte, not approximately).
2. The RLE decoder matches a hand-computed fixture (e.g. a literal run
   plus a repeat run in one row) — pin the exact expected bytes.
3. Hardness/spacing percentages map to 0..=1 fractions (0.5 → 0.5).
4. Wrong magic bytes → the named "not an .abr" error (not a panic).
5. A truncated file (cut in the middle of the alpha data) → the named
   malformed/truncated error, never a panic and never a partial success.
6. A stamp whose declared size exceeds the 4096x4096 cap → the cap error,
   and the test asserts no huge allocation happened (the error is returned
   before the buffer is sized).
7. A v8+ (descriptor-only) file → the unsupported-version error naming the
   version.

## Style anchors (match the crate)

- Error enums in this crate use `thiserror` (`#[derive(Debug, Error)]` with
  `#[error("...")]` messages). See `crates/umber-brush/src/preset.rs` for
  the exact convention (`PresetError`).
- Module docs at the top: a short `//!` header explaining what the module
  is, the format scope (v6/v7 samp only), the clean-room provenance
  (ported from the vendored ag-psd reference), and the named non-goals
  (descriptor/desc parsing, writing .abr — never).
- Doc comments on every public item (`///`), with `# Errors` sections on
  fallible public functions.

## Handoff

When done, write your summary to the file `.dragon/briefs/abr-core-result.md`
in this worktree (create the directory if needed). The file must contain:
what you implemented, every pin you wrote and what it asserts, any
assumption you made that the dragon must verify, anything you deliberately
left out, and the exact public API signatures you landed (the dragon
wires them). Keep it under 400 lines.

Do not commit. The dragon commits after its gates pass.
