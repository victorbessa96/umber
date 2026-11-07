//! Clean-room reader for Adobe Photoshop `.abr` brush files, **v6/v7
//! `samp` sections only**.
//!
//! Provenance: the section walk and the PackBits/RLE decoder are a
//! clean-room port of the *logic* in the vendored `ag-psd` reference
//! (`docs/claw-artifacts/ag-psd/src-tree/src/abr.rs`, `read_abr` and
//! `read_data_rle`). No reference code, comments or prose were copied.
//! That reference is this module's format authority: where it disagrees
//! with a prose description, the reference wins. Two consequences are
//! worth stating up front because they are easy to get wrong:
//!
//! * every multi-byte field is **big-endian** (the reference reads all of
//!   them with `from_be_bytes`, the PSD-family convention);
//! * the byte after the 16-bit bit depth is a **one-byte compression
//!   code** (0 = raw, 1 = PackBits/RLE), not a 16-bit row count.
//!
//! Supported: version 6 and 7 (minor version 1 or 2), the `8BIM` section
//! walk, `samp` sampled-stamp records, 8-bit raw and RLE alpha, and
//! 16-bit raw alpha (the high byte is kept). Every other section (`desc`,
//! `patt`, `phry`, unknown tags) is skipped by its declared length, so a
//! normal v6/v7 file parses even though only `samp` is interpreted.
//!
//! Deliberate non-goals (the v1 boundary):
//!
//! * descriptor (`desc`) parsing: brush *settings* (dynamics, texture,
//!   dual brush, ...) are a separate kill;
//! * `patt` pattern decoding;
//! * v8+ files, which are descriptor-only and rejected as unsupported;
//! * 16-bit RLE (rejected with a named error);
//! * writing `.abr` — never (the format is read-only for umber).
//!
//! Safety caps (hostile input): at most [`MAX_BRUSHES`] brushes per file,
//! and at most [`MAX_STAMP_DIMENSION`] x [`MAX_STAMP_DIMENSION`] pixels per
//! stamp. Both are checked *before* the stamp plane is allocated, so a
//! hostile header cannot trigger a huge allocation.
//!
//! Judgment calls:
//!
//! * The per-brush parameter block between the name and the bounds is 10
//!   bytes at minor version 1 and 264 bytes at minor version 2. The
//!   reference implementation SKIPS the whole block without naming its
//!   fields, and the brush's real settings (diameter, hardness,
//!   spacing, angle, dynamics) live in the descriptor (`desc`) sections,
//!   which this parser does not read. The block is therefore skipped,
//!   never interpreted: publishing bytes as parameters would report
//!   numbers that are not the brush's. The descriptor wave is the named
//!   follow-up.
//! * PackBits header byte 128 is a no-op: the reference ignores it. It
//!   does **not** mean "repeat the next byte three times".
//! * A row whose RLE stream decodes fewer than `width` bytes leaves the
//!   remainder of that row zero (the reference's behavior). A stream that
//!   claims bytes past the end of its row is a malformed error.
//! * Brush names are decoded byte-for-byte as Latin-1 (each byte becomes
//!   the code point of the same value), matching the reference's
//!   `read_short_string`; name decoding can never fail.
//! * A v6/v7 file with no `samp` section parses to an empty brush list
//!   rather than an error: the section walk is length-delimited, so the
//!   absence of stamps is not corruption.

use thiserror::Error;

/// Maximum number of brushes accepted from one file.
pub const MAX_BRUSHES: usize = 4096;

/// Maximum accepted stamp width and height, in pixels.
pub const MAX_STAMP_DIMENSION: u32 = 4096;

/// The maximum TOTAL decoded stamp data one file may produce, in bytes.
///
/// The per-stamp cap alone still allows 4096 stamps of 16 MiB each — 64
/// GiB of decoded planes out of a small hostile file — so the sum is
/// capped too.
pub const MAX_TOTAL_ALPHA_BYTES: usize = 256 * 1024 * 1024;

/// One parsed brush from an `.abr` file.
#[derive(Debug, Clone)]
pub struct AbrBrush {
    /// The brush name, decoded byte-for-byte as Latin-1.
    pub name: String,
    /// Stamp width in pixels.
    pub width: u32,
    /// Stamp height in pixels.
    pub height: u32,
    /// 8-bit alpha plane, row-major, exactly `width * height` bytes.
    pub alpha: Vec<u8>,
}

/// Errors [`read_abr`] can return.
///
/// Every failure is a named variant: the parser never panics on hostile
/// input, and the messages are suitable for showing to a user.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AbrError {
    /// The bytes are not an `.abr` file: the version field is absent or is
    /// not a plausible ABR version, or a section is missing the `8BIM`
    /// signature.
    #[error("not an .abr brush file: missing the 8BIM section signature")]
    NotAbr,
    /// A recognized but unsupported ABR version (v1–v5, or the
    /// descriptor-only v8 and later).
    #[error("unsupported .abr version {version}: only v6 and v7 sampled brushes are supported")]
    UnsupportedVersion {
        /// The version field as stored in the file.
        version: i16,
    },
    /// A v6/v7 file whose minor version is neither 1 nor 2.
    #[error("unsupported .abr minor version {minor}: only 1 and 2 are supported")]
    UnsupportedMinorVersion {
        /// The minor-version field as stored in the file.
        minor: i16,
    },
    /// A structurally invalid or truncated file.
    #[error("malformed .abr brush file: {detail}")]
    Malformed {
        /// What failed, in user-facing terms.
        detail: String,
    },
    /// More than [`MAX_BRUSHES`] brush records.
    #[error("too many brushes ({count}): the safety cap is {max} per file")]
    TooManyBrushes {
        /// The count that tripped the cap (one past the maximum).
        count: usize,
        /// The cap itself.
        max: usize,
    },
    /// The file's decoded stamp planes exceed [`MAX_TOTAL_ALPHA_BYTES`].
    #[error("too much stamp data ({bytes} bytes): the safety cap is {max} bytes per file")]
    TooMuchData {
        /// The decoded byte total that tripped the cap.
        bytes: usize,
        /// The cap itself.
        max: usize,
    },
    /// A stamp whose declared bounds exceed [`MAX_STAMP_DIMENSION`].
    #[error("brush stamp {width}x{height} exceeds the {max}x{max} pixel safety cap")]
    StampTooLarge {
        /// Declared stamp width in pixels.
        width: u32,
        /// Declared stamp height in pixels.
        height: u32,
        /// The cap itself, in pixels per side.
        max: u32,
    },
    /// A bit depth other than 8, or 16-bit RLE data.
    #[error("unsupported brush bit depth {depth}: only 8-bit and 16-bit raw stamps are supported")]
    UnsupportedBitDepth {
        /// The bit depth as stored in the file.
        depth: i16,
    },
    /// A compression code other than 0 (raw) or 1 (RLE), or 16-bit RLE.
    #[error(
        "unsupported brush compression {compression}: 0 (raw) and 1 (RLE) are supported for 8-bit stamps, 0 for 16-bit"
    )]
    UnsupportedCompression {
        /// The compression code as stored in the file.
        compression: u8,
    },
}

/// Parses every brush in an `.abr` file.
///
/// Only v6/v7 `samp` sections are interpreted; other sections are skipped
/// by their declared length. The returned brushes are in file order.
///
/// # Errors
///
/// * [`AbrError::NotAbr`] when the header or a section signature is not
///   recognizable as an `.abr` file.
/// * [`AbrError::UnsupportedVersion`] / [`AbrError::UnsupportedMinorVersion`]
///   for versions outside v6/v7 minor 1–2.
/// * [`AbrError::TooManyBrushes`] / [`AbrError::StampTooLarge`] when a
///   safety cap is exceeded (before any stamp plane is allocated).
/// * [`AbrError::Malformed`] for truncated or structurally invalid data.
/// * [`AbrError::UnsupportedBitDepth`] / [`AbrError::UnsupportedCompression`]
///   for stamp encodings this reader does not decode.
pub fn read_abr(bytes: &[u8]) -> Result<Vec<AbrBrush>, AbrError> {
    if bytes.len() < 2 {
        return Err(AbrError::NotAbr);
    }

    let mut reader = Reader::new(bytes);
    let version = reader.i16()?;
    if !(6..=7).contains(&version) {
        return if version >= 1 {
            Err(AbrError::UnsupportedVersion { version })
        } else {
            Err(AbrError::NotAbr)
        };
    }

    let minor = reader.i16()?;
    if minor != 1 && minor != 2 {
        return Err(AbrError::UnsupportedMinorVersion { minor });
    }
    let parameter_len = if minor == 1 { 10 } else { 264 };

    let mut brushes = Vec::new();
    while reader.remaining() > 0 {
        if reader.remaining() < 12 {
            return Err(malformed("truncated section header"));
        }

        let signature = reader.take(4)?;
        if signature != b"8BIM" {
            return Err(AbrError::NotAbr);
        }
        let kind = reader.take(4)?;
        let size = usize::try_from(reader.u32()?)
            .map_err(|_| malformed("section size does not fit in memory"))?;
        let payload = reader.take(size)?;

        if kind == b"samp" {
            parse_samp(payload, parameter_len, &mut brushes)?;
        }

        // Sections are padded to a four-byte boundary. The trailing padding
        // may be absent at end-of-file, so clamp instead of failing.
        let pad = (4 - (size % 4)) % 4;
        let skip = pad.min(reader.remaining());
        reader.skip(skip)?;
    }

    Ok(brushes)
}

/// Parses the concatenated brush records of one `samp` payload.
fn parse_samp(
    payload: &[u8],
    parameter_len: usize,
    brushes: &mut Vec<AbrBrush>,
) -> Result<(), AbrError> {
    parse_samp_with_budget(payload, parameter_len, brushes, MAX_TOTAL_ALPHA_BYTES)
}

/// [`parse_samp`] with an explicit total-plane budget (the test seam for
/// the cap; production always passes [`MAX_TOTAL_ALPHA_BYTES`]).
fn parse_samp_with_budget(
    payload: &[u8],
    parameter_len: usize,
    brushes: &mut Vec<AbrBrush>,
    max_total_bytes: usize,
) -> Result<(), AbrError> {
    let mut reader = Reader::new(payload);
    let mut total_bytes = 0usize;
    while reader.remaining() > 0 {
        if brushes.len() >= MAX_BRUSHES {
            return Err(AbrError::TooManyBrushes {
                count: brushes.len() + 1,
                max: MAX_BRUSHES,
            });
        }
        if reader.remaining() < 4 {
            return Err(malformed("truncated brush record length"));
        }

        let declared = usize::try_from(reader.u32()?)
            .map_err(|_| malformed("brush record length does not fit in memory"))?;
        if declared == 0 {
            return Err(malformed("zero-length brush record"));
        }
        let pad = (4 - (declared % 4)) % 4;
        let padded = declared
            .checked_add(pad)
            .ok_or_else(|| malformed("brush record length overflow"))?;
        let body = reader.take(padded)?;
        let brush = parse_brush(body, parameter_len)?;
        // The total-plane budget: the per-stamp cap alone still allows
        // 4096 stamps of 16 MiB each (64 GiB of decoded planes from a
        // small hostile file), so the SUM is capped too.
        total_bytes = total_bytes
            .checked_add(brush.alpha.len())
            .ok_or_else(|| malformed("total stamp data size overflow"))?;
        if total_bytes > max_total_bytes {
            return Err(AbrError::TooMuchData {
                bytes: total_bytes,
                max: max_total_bytes,
            });
        }
        brushes.push(brush);
    }
    Ok(())
}

/// Parses one brush record body (everything after its length prefix).
fn parse_brush(body: &[u8], parameter_len: usize) -> Result<AbrBrush, AbrError> {
    let mut reader = Reader::new(body);

    let name_len = usize::from(reader.u8()?);
    let name_bytes = reader.take(name_len)?;
    let name: String = name_bytes.iter().map(|&byte| byte as char).collect();

    // The v6.1 tip-shape block is five big-endian u16s; v6.2 widens the
    // block to 264 bytes but (per the module docs) keeps the same prefix.
    // The block is SKIPPED, not interpreted: the reference implementation
    // skips it without naming its fields, and the brush's real settings
    // (diameter, hardness, spacing, angle) live in the descriptor
    // (`desc`) sections, which this parser does not read yet (the named
    // follow-up wave). Reading the padding as parameters would publish
    // numbers that are not the brush's, so this parser refuses to guess.
    let _parameters = reader.take(parameter_len)?;

    // Bounds: y, x, bottom, right. Height and width are the spans.
    let y = i64::from(reader.i32()?);
    let x = i64::from(reader.i32()?);
    let bottom = i64::from(reader.i32()?);
    let right = i64::from(reader.i32()?);
    let height_span = bottom - y;
    let width_span = right - x;
    if width_span <= 0 || height_span <= 0 {
        return Err(malformed("brush stamp has non-positive bounds"));
    }

    let width =
        u32::try_from(width_span).map_err(|_| malformed("brush stamp width is out of range"))?;
    let height =
        u32::try_from(height_span).map_err(|_| malformed("brush stamp height is out of range"))?;
    if width > MAX_STAMP_DIMENSION || height > MAX_STAMP_DIMENSION {
        return Err(AbrError::StampTooLarge {
            width,
            height,
            max: MAX_STAMP_DIMENSION,
        });
    }

    // The cap is enforced above, so this allocation is bounded.
    let width_px = usize::try_from(width)
        .map_err(|_| malformed("brush stamp width does not fit in memory"))?;
    let height_px = usize::try_from(height)
        .map_err(|_| malformed("brush stamp height does not fit in memory"))?;
    let pixel_count = width_px
        .checked_mul(height_px)
        .ok_or_else(|| malformed("brush stamp size overflow"))?;
    let mut alpha = vec![0u8; pixel_count];

    let bit_depth = reader.i16()?;
    let compression = reader.u8()?;
    match (bit_depth, compression) {
        (8, 0) => {
            let raw = reader.take(pixel_count)?;
            alpha.copy_from_slice(raw);
        }
        (8, 1) => decode_rle(&mut reader, width_px, height_px, &mut alpha)?,
        (16, 0) => {
            let byte_count = pixel_count
                .checked_mul(2)
                .ok_or_else(|| malformed("16-bit brush stamp size overflow"))?;
            let raw = reader.take(byte_count)?;
            for (pixel, sample) in alpha.iter_mut().zip(raw.chunks_exact(2)) {
                if let [high, _low] = sample {
                    *pixel = *high;
                }
            }
        }
        (8 | 16, other) => {
            return Err(AbrError::UnsupportedCompression { compression: other });
        }
        (depth, _) => return Err(AbrError::UnsupportedBitDepth { depth }),
    }

    Ok(AbrBrush {
        name,
        width,
        height,
        alpha,
    })
}

/// Decodes a one-plane, 8-bit RLE stamp.
///
/// The row length table (one big-endian `u16` per row) precedes the packed
/// rows. Each row decodes into `width` bytes; short rows leave zeros.
fn decode_rle(
    reader: &mut Reader<'_>,
    width: usize,
    height: usize,
    alpha: &mut [u8],
) -> Result<(), AbrError> {
    let mut lengths = Vec::with_capacity(height);
    for _ in 0..height {
        lengths.push(usize::from(reader.u16()?));
    }

    for (row, &encoded_len) in alpha.chunks_exact_mut(width).zip(lengths.iter()) {
        let encoded = reader.take(encoded_len)?;
        decode_packbits_row(encoded, row)?;
    }
    Ok(())
}

/// Decodes one PackBits row into `out` (which is the row's `width` slice).
///
/// Header `0..=127` is a literal run of `header + 1` bytes; `129..=255`
/// repeats the next byte `257 - header` times; `128` is a no-op. This is
/// exactly the reference's `read_data_rle` behavior.
fn decode_packbits_row(encoded: &[u8], out: &mut [u8]) -> Result<(), AbrError> {
    let mut index = 0usize;
    let mut written = 0usize;

    while let Some(&header) = encoded.get(index) {
        index += 1;
        match header {
            0..=127 => {
                let run = usize::from(header) + 1;
                let end = index
                    .checked_add(run)
                    .ok_or_else(|| malformed("RLE literal run overflow"))?;
                let literal = encoded
                    .get(index..end)
                    .ok_or_else(|| malformed("truncated RLE literal run"))?;
                let take = run.min(out.len().saturating_sub(written));
                if let Some(destination) = out.get_mut(written..written + take) {
                    if let Some(source) = literal.get(..take) {
                        destination.copy_from_slice(source);
                    }
                }
                written += take;
                index = end;
            }
            129..=255 => {
                let Some(&value) = encoded.get(index) else {
                    return Err(malformed("truncated RLE repeat run"));
                };
                index += 1;
                let run = 257 - usize::from(header);
                let take = run.min(out.len().saturating_sub(written));
                if let Some(destination) = out.get_mut(written..written + take) {
                    destination.fill(value);
                }
                written += take;
            }
            _ => {
                // 128: PackBits' no-op; consume only the header byte.
            }
        }
    }

    Ok(())
}

/// Builds a [`AbrError::Malformed`] with a static, user-facing detail.
fn malformed(detail: impl Into<String>) -> AbrError {
    AbrError::Malformed {
        detail: detail.into(),
    }
}

/// A bounds-checked, big-endian cursor over the input bytes.
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], AbrError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| malformed("record length overflow"))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| malformed("unexpected end of file"))?;
        self.offset = end;
        Ok(slice)
    }

    fn skip(&mut self, len: usize) -> Result<(), AbrError> {
        self.take(len).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8, AbrError> {
        match self.take(1)? {
            [byte] => Ok(*byte),
            _ => Err(malformed("unexpected end of file")),
        }
    }

    fn u16(&mut self) -> Result<u16, AbrError> {
        match self.take(2)? {
            [high, low] => Ok(u16::from_be_bytes([*high, *low])),
            _ => Err(malformed("unexpected end of file")),
        }
    }

    fn i16(&mut self) -> Result<i16, AbrError> {
        match self.take(2)? {
            [high, low] => Ok(i16::from_be_bytes([*high, *low])),
            _ => Err(malformed("unexpected end of file")),
        }
    }

    fn u32(&mut self) -> Result<u32, AbrError> {
        match self.take(4)? {
            [a, b, c, d] => Ok(u32::from_be_bytes([*a, *b, *c, *d])),
            _ => Err(malformed("unexpected end of file")),
        }
    }

    fn i32(&mut self) -> Result<i32, AbrError> {
        match self.take(4)? {
            [a, b, c, d] => Ok(i32::from_be_bytes([*a, *b, *c, *d])),
            _ => Err(malformed("unexpected end of file")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal big-endian writer for the synthetic fixtures.
    struct Builder {
        bytes: Vec<u8>,
    }

    impl Builder {
        fn new() -> Self {
            Self { bytes: Vec::new() }
        }

        fn u8(&mut self, value: u8) {
            self.bytes.push(value);
        }

        fn u16(&mut self, value: u16) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn i16(&mut self, value: i16) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn u32(&mut self, value: u32) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn i32(&mut self, value: i32) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn raw(&mut self, bytes: &[u8]) {
            self.bytes.extend_from_slice(bytes);
        }

        fn pascal(&mut self, text: &str) {
            let bytes = text.as_bytes();
            let len = u8::try_from(bytes.len()).expect("test brush name fits a u8");
            self.u8(len);
            self.raw(bytes);
        }

        fn into_vec(self) -> Vec<u8> {
            self.bytes
        }
    }

    /// The variable parts of one synthetic sampled brush.
    struct Stamp<'a> {
        name: &'a str,
        width: i32,
        height: i32,
        depth: i16,
        compression: u8,
        payload: &'a [u8],
        hardness_percent: u16,
        spacing_percent: u16,
        diameter: u16,
    }

    impl<'a> Stamp<'a> {
        fn new(name: &'a str, width: i32, height: i32, payload: &'a [u8]) -> Self {
            Self {
                name,
                width,
                height,
                depth: 8,
                compression: 1,
                payload,
                hardness_percent: 100,
                spacing_percent: 25,
                diameter: 0,
            }
        }
    }

    /// Encodes a minor-1 brush record: length prefix, name, the 10-byte
    /// parameter block, bounds, depth, compression, alpha payload.
    fn brush_record(stamp: &Stamp<'_>) -> Vec<u8> {
        let mut body = Builder::new();
        body.pascal(stamp.name);
        body.u16(0); // roundness (ignored by the parser)
        body.u16(0); // angle (ignored by the parser)
        body.u16(stamp.hardness_percent);
        body.u16(stamp.spacing_percent);
        body.u16(stamp.diameter);
        body.i32(0); // y
        body.i32(0); // x
        body.i32(stamp.height); // bottom
        body.i32(stamp.width); // right
        body.i16(stamp.depth);
        body.u8(stamp.compression);
        body.raw(stamp.payload);

        let mut body = body.into_vec();
        let declared = u32::try_from(body.len()).expect("test brush body fits a u32");
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }

        let mut record = Builder::new();
        record.u32(declared);
        record.raw(&body);
        record.into_vec()
    }

    /// Wraps section payloads in a v6 file with the given minor version.
    fn abr_file(minor: i16, sections: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut file = Builder::new();
        file.i16(6);
        file.i16(minor);
        for (kind, payload) in sections {
            file.raw(b"8BIM");
            file.raw(*kind);
            file.u32(u32::try_from(payload.len()).expect("test payload fits a u32"));
            file.raw(payload);
            let pad = (4 - payload.len() % 4) % 4;
            for _ in 0..pad {
                file.u8(0);
            }
        }
        file.into_vec()
    }

    /// Builds the RLE payload: one big-endian u16 length per row, then the
    /// packed rows.
    fn rle_payload(rows: &[Vec<u8>]) -> Vec<u8> {
        let mut payload = Builder::new();
        for row in rows {
            payload.u16(u16::try_from(row.len()).expect("test row fits a u16"));
        }
        for row in rows {
            payload.raw(row);
        }
        payload.into_vec()
    }

    /// Pin 1: a synthetic v6 file yields exactly the written brush, with
    /// the alpha plane equal byte-for-byte.
    #[test]
    fn reads_a_synthetic_v6_brush_byte_for_byte() {
        let alpha = [10u8, 20, 30, 40, 50, 60, 70, 80];
        let rows = vec![
            vec![3u8, 10, 20, 30, 40], // literal run of four
            vec![3u8, 50, 60, 70, 80],
        ];
        let payload = rle_payload(&rows);

        let mut stamp = Stamp::new("Test Tip", 4, 2, &payload);
        stamp.hardness_percent = 50;
        stamp.spacing_percent = 25;
        stamp.diameter = 12;
        let record = brush_record(&stamp);
        let file = abr_file(1, &[(b"samp", record)]);

        let brushes = read_abr(&file).expect("synthetic v6 file parses");
        assert_eq!(brushes.len(), 1);
        let brush = &brushes[0];
        assert_eq!(brush.name, "Test Tip");
        assert_eq!(brush.width, 4);
        assert_eq!(brush.height, 2);
        assert_eq!(brush.alpha, alpha);
    }

    /// Pin 2: one row mixing a literal run and a repeat run decodes to the
    /// hand-computed bytes.
    #[test]
    fn packbits_literal_plus_repeat_matches_hand_computed_bytes() {
        // header 2 = three literal bytes; header 0xFE = repeat three times.
        let rows = vec![vec![2u8, 1, 2, 3, 0xFE, 9]];
        let payload = rle_payload(&rows);
        let stamp = Stamp::new("Rle", 6, 1, &payload);
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        let brushes = read_abr(&file).expect("RLE fixture parses");
        assert_eq!(brushes[0].alpha, vec![1, 2, 3, 9, 9, 9]);
    }

    /// Pin 3: the per-brush parameter block is SKIPPED, never published —
    /// the brush's real settings live in the descriptor sections, which
    /// this parser does not read, so whatever bytes sit in the block
    /// cannot change the parsed stamp.
    #[test]
    fn parameter_block_is_skipped_not_interpreted() {
        let alpha = [128u8];
        let mut stamp = Stamp::new("Half", 1, 1, &alpha);
        stamp.compression = 0;
        stamp.hardness_percent = 50;
        stamp.spacing_percent = 50;
        stamp.diameter = 9;
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        let brushes = read_abr(&file).expect("percent fixture parses");
        // The stamp is exact; the block's numbers are NOT exposed.
        assert_eq!(brushes[0].name, "Half");
        assert_eq!(brushes[0].width, 1);
        assert_eq!(brushes[0].height, 1);
        assert_eq!(brushes[0].alpha, alpha);
        // A file whose block holds different bytes parses to the SAME
        // stamp: the block carries no published data.
        let mut other = Stamp::new("Half", 1, 1, &alpha);
        other.compression = 0;
        other.hardness_percent = 250;
        other.spacing_percent = 250;
        other.diameter = 4096;
        let other_file = abr_file(1, &[(b"samp", brush_record(&other))]);
        let other_brushes = read_abr(&other_file).expect("variant fixture parses");
        assert_eq!(other_brushes[0].alpha, brushes[0].alpha);
        assert_eq!(other_brushes[0].width, brushes[0].width);
    }

    /// Pin 4: wrong magic bytes give the named "not an .abr" error.
    #[test]
    fn wrong_magic_is_not_an_abr() {
        assert_eq!(read_abr(&[]).unwrap_err(), AbrError::NotAbr);
        assert_eq!(
            read_abr(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap_err(),
            AbrError::NotAbr
        );

        // A plausible v6 header followed by a corrupt section signature.
        let mut file = Builder::new();
        file.i16(6);
        file.i16(1);
        file.raw(b"8BAD");
        file.raw(b"samp");
        file.u32(0);
        assert_eq!(read_abr(&file.into_vec()).unwrap_err(), AbrError::NotAbr);
    }

    /// Pin 5: a file cut inside the alpha payload is malformed, never a
    /// panic and never a partial success.
    #[test]
    fn truncated_alpha_data_is_malformed() {
        let rows = vec![vec![3u8, 1, 2, 3, 4]];
        let payload = rle_payload(&rows);
        let stamp = Stamp::new("Cut", 4, 1, &payload);
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        // Drop the last four bytes (the tail of the alpha payload plus the
        // record padding). Length-delimited parsing reports the truncation
        // before it can produce a brush.
        let cut = file.len() - 4;
        let error = read_abr(&file[..cut]).unwrap_err();
        assert!(
            matches!(error, AbrError::Malformed { .. }),
            "expected Malformed, got {error:?}"
        );
    }

    /// Pin 6: an oversized declared stamp trips the cap before the plane is
    /// sized. The declared size here is 100_000 x 100_000 = 10 GB, so a
    /// pre-allocation check is the only way this test can finish.
    #[test]
    fn oversized_stamp_is_rejected_before_allocating() {
        let mut body = Builder::new();
        body.pascal("Huge");
        body.u16(0);
        body.u16(0);
        body.u16(100);
        body.u16(25);
        body.u16(9);
        body.i32(0); // y
        body.i32(0); // x
        body.i32(100_000); // bottom -> height
        body.i32(100_000); // right  -> width
        body.i16(8);
        body.u8(1); // RLE, but the cap must fire before any alpha read
        let mut body = body.into_vec();
        let declared = u32::try_from(body.len()).expect("test body fits a u32");
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }
        let mut record = Builder::new();
        record.u32(declared);
        record.raw(&body);
        let file = abr_file(1, &[(b"samp", record.into_vec())]);

        let error = read_abr(&file).unwrap_err();
        assert_eq!(
            error,
            AbrError::StampTooLarge {
                width: 100_000,
                height: 100_000,
                max: MAX_STAMP_DIMENSION,
            }
        );
    }

    /// Pin 7: a v8+ descriptor-only file names its version.
    #[test]
    fn descriptor_only_v8_is_unsupported() {
        let mut file = Builder::new();
        file.i16(8);
        file.i16(2);
        file.raw(b"8BIM");
        file.raw(b"desc");
        file.u32(0);

        assert_eq!(
            read_abr(&file.into_vec()).unwrap_err(),
            AbrError::UnsupportedVersion { version: 8 }
        );
    }

    /// v1/v2/v5 (and every other pre-v6 version) are named unsupported.
    #[test]
    fn older_versions_are_unsupported() {
        for version in [1i16, 2, 5] {
            let mut file = Builder::new();
            file.i16(version);
            file.i16(1);
            assert_eq!(
                read_abr(&file.into_vec()).unwrap_err(),
                AbrError::UnsupportedVersion { version }
            );
        }
    }

    /// A minor version outside 1..=2 is named unsupported.
    #[test]
    fn unknown_minor_version_is_unsupported() {
        let mut file = Builder::new();
        file.i16(6);
        file.i16(3);
        assert_eq!(
            read_abr(&file.into_vec()).unwrap_err(),
            AbrError::UnsupportedMinorVersion { minor: 3 }
        );
    }

    /// Raw 8-bit alpha is copied verbatim.
    #[test]
    fn raw_alpha_is_copied_verbatim() {
        let alpha = [7u8, 8, 9, 10, 11, 12];
        let mut stamp = Stamp::new("Raw", 3, 2, &alpha);
        stamp.compression = 0;
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        let brushes = read_abr(&file).expect("raw fixture parses");
        assert_eq!(brushes[0].alpha, alpha);
        assert_eq!(brushes[0].width, 3);
        assert_eq!(brushes[0].height, 2);
    }

    /// 16-bit raw samples keep their high byte (big-endian).
    #[test]
    fn sixteen_bit_raw_keeps_the_high_byte() {
        let raw = [0x12u8, 0x34, 0xAB, 0xCD];
        let mut stamp = Stamp::new("Deep", 2, 1, &raw);
        stamp.compression = 0;
        stamp.depth = 16;
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        let brushes = read_abr(&file).expect("16-bit fixture parses");
        assert_eq!(brushes[0].alpha, vec![0x12, 0xAB]);
    }

    /// PackBits header 128 is a no-op, matching the reference.
    #[test]
    fn packbits_128_is_a_noop() {
        let rows = vec![vec![2u8, 1, 2, 3, 0x80, 0xFE, 9]];
        let payload = rle_payload(&rows);
        let stamp = Stamp::new("Noop", 6, 1, &payload);
        let file = abr_file(1, &[(b"samp", brush_record(&stamp))]);

        let brushes = read_abr(&file).expect("no-op fixture parses");
        assert_eq!(brushes[0].alpha, vec![1, 2, 3, 9, 9, 9]);
    }

    /// Non-`samp` sections are skipped by their declared length, so a
    /// descriptor section before the stamps does not break the read.
    #[test]
    fn descriptor_section_is_skipped() {
        let descriptor = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02];
        let alpha = [200u8];
        let mut stamp = Stamp::new("After Desc", 1, 1, &alpha);
        stamp.compression = 0;
        let record = brush_record(&stamp);
        let file = abr_file(1, &[(b"desc", descriptor), (b"samp", record)]);

        let brushes = read_abr(&file).expect("desc-prefixed file parses");
        assert_eq!(brushes.len(), 1);
        assert_eq!(brushes[0].name, "After Desc");
        assert_eq!(brushes[0].alpha, vec![200]);
    }

    /// The brush-count cap fires at 4097 records.
    #[test]
    fn more_than_max_brushes_is_rejected() {
        let mut payload = Vec::new();
        for index in 0..=MAX_BRUSHES {
            let name = format!("b{index}");
            let alpha = [index as u8];
            let mut stamp = Stamp::new(&name, 1, 1, &alpha);
            stamp.compression = 0;
            payload.extend_from_slice(&brush_record(&stamp));
        }
        let file = abr_file(1, &[(b"samp", payload)]);

        assert_eq!(
            read_abr(&file).unwrap_err(),
            AbrError::TooManyBrushes {
                count: MAX_BRUSHES + 1,
                max: MAX_BRUSHES,
            }
        );
    }

    /// A minor-2 record is structurally identical, with a 264-byte
    /// parameter block.
    #[test]
    fn minor_two_reads_the_parameter_prefix() {
        let mut body = Builder::new();
        body.pascal("V62");
        body.u16(0); // roundness
        body.u16(0); // angle
        body.u16(40); // hardness %
        body.u16(30); // spacing %
        body.u16(18); // diameter
        for _ in 0..254 {
            body.u8(0); // the rest of the v6.2 block
        }
        body.i32(0);
        body.i32(0);
        body.i32(1);
        body.i32(1);
        body.i16(8);
        body.u8(0);
        body.u8(77);
        let mut body = body.into_vec();
        let declared = u32::try_from(body.len()).expect("test body fits a u32");
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }
        let mut record = Builder::new();
        record.u32(declared);
        record.raw(&body);
        let file = abr_file(2, &[(b"samp", record.into_vec())]);

        let brushes = read_abr(&file).expect("minor-2 fixture parses");
        assert_eq!(brushes[0].name, "V62");
        assert_eq!(brushes[0].alpha, vec![77]);
        assert_eq!(brushes[0].width, 1);
        assert_eq!(brushes[0].height, 1);
    }

    /// The total-plane budget refuses a file whose decoded stamps exceed
    /// the cap (the per-stamp cap alone allows 64 GiB across 4096
    /// stamps).
    #[test]
    fn total_data_budget_refuses_the_sum() {
        let alpha = [0u8, 1, 2, 3];
        let mut stamp = Stamp::new("Hot", 4, 1, &alpha);
        stamp.compression = 0;
        let payload = brush_record(&stamp);
        let mut brushes = Vec::new();
        let err = parse_samp_with_budget(&payload, 10, &mut brushes, 3)
            .expect_err("the sum over the budget is refused");
        assert_eq!(err, AbrError::TooMuchData { bytes: 4, max: 3 });
        assert!(brushes.is_empty(), "the refused brush is not published");
    }

    /// Version 7 shares the v6 `samp` layout.
    #[test]
    fn version_seven_is_accepted() {
        let alpha = [5u8];
        let mut stamp = Stamp::new("V7", 1, 1, &alpha);
        stamp.compression = 0;
        let mut file = abr_file(1, &[(b"samp", brush_record(&stamp))]);
        file[0..2].copy_from_slice(&7i16.to_be_bytes());

        let brushes = read_abr(&file).expect("v7 fixture parses");
        assert_eq!(brushes[0].name, "V7");
        assert_eq!(brushes[0].alpha, vec![5]);
    }

    /// A zero-length brush record is rejected instead of looping forever.
    #[test]
    fn zero_length_brush_record_is_malformed() {
        let mut payload = Builder::new();
        payload.u32(0);
        let file = abr_file(1, &[(b"samp", payload.into_vec())]);

        let error = read_abr(&file).unwrap_err();
        assert!(
            matches!(error, AbrError::Malformed { .. }),
            "expected Malformed, got {error:?}"
        );
    }
}
