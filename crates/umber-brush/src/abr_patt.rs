//! Clean-room decoder for the `patt` (tiling pattern) section of a
//! Photoshop `.abr` brush file — wave 3.1a of
//! `docs/research/2026-11-09-abr-patt-analysis.md`.
//!
//! A `patt` section is a list of tiling images with **no alpha**. In a
//! brush file each one is a brush **texture**: a brush whose descriptor sets
//! `useTexture` names a pattern by id (`Txtr.Idnt`), and the pattern is
//! tiled across the canvas and blended into every dab. [`AbrPattern`] is the
//! decoded image ([`crate::abr::AbrBrush::texture`] is the link, and
//! [`crate::abr::match_pattern`] resolves it).
//!
//! Provenance: the record layout is clean-room, assembled from public
//! documentation of the PSD-family pattern data and exploration results
//! assembled from online sources. No vendor code, comments or prose were
//! copied. Every multi-byte field is big-endian, the PSD-family convention.
//!
//! One record, in field order:
//!
//! * `u32` record length (excludes itself; the record is padded to a
//!   four-byte boundary);
//! * `u32` record version (must be 1);
//! * `u32` image mode (1 = grayscale, 2 = indexed, 3 = RGB);
//! * `i16`, `i16` the pattern's size as (vertical, horizontal) — it
//!   duplicates the rectangle and is not used for sizing;
//! * `u32` code-unit count + big-endian UTF-16BE units: the name (a
//!   trailing NUL unit is dropped);
//! * a Pascal string (one-byte length, Latin-1): the pattern id, the link
//!   key a brush's `Txtr.Idnt` names;
//! * indexed mode only: 256 x 3 palette bytes, then four skipped bytes;
//! * `u32` pixel-array version (must be 3), then a `u32` length that is
//!   read and ignored;
//! * four `u32` rectangle values: top, left, bottom, right; the size is
//!   (right - left) x (bottom - top);
//! * `u32` channel count; the slot loop runs `count + 2` slots (the two
//!   extras are the mask slots of the public layout);
//! * each slot: a `u32` written flag (0 = empty, nothing else follows), then
//!   a `u32` length, a `u32` pixel depth, four `u32` channel-rectangle
//!   values, the depth repeated as a `u16`, a one-byte compression code and
//!   `length - 23` bytes of sample data (4 + 16 + 2 + 1 = 23).
//!
//! Written slots map to pixels through a *written-slot* counter: RGB takes
//! slots 0/1/2 as R/G/B, grayscale takes slot 0 replicated to R/G/B, indexed
//! takes a palette lookup, and later (mask) slots are decoded but not
//! blitted. Output is RGBA8 with alpha fixed at 255.
//!
//! ## Soft decoding — deliberately unlike `desc`
//!
//! A `patt` section has been accepted by *skipping* since row 268, so no
//! pattern problem may turn a file that imports today into a refusal. A
//! record this decoder cannot read is skipped by its declared length and
//! counted in [`crate::abr::AbrFile::skipped_patterns`]; a record whose
//! declared length runs past the end of the section ends pattern parsing for
//! that section and is counted once. [`parse_patterns`] therefore never
//! returns an error: a hostile `patt` payload costs patterns, never the
//! file. The `samp` and `desc` readers stay strict.
//!
//! ## Full mode only
//!
//! [`crate::abr::read_samples`] keeps skipping `patt` by length: the
//! samples-only entry point is pinned to row 268's behavior, exactly as it
//! skips `desc`/`phry` (see the `abr` module docs).
//!
//! ## Safety caps (hostile input), each checked BEFORE its allocation
//!
//! * [`MAX_PATTERN_DIMENSION`] per side — the pattern rectangle, and the
//!   channel rectangle, which must also lie inside the pattern;
//! * [`MAX_PATTERNS`] patterns per file — further records are skipped and
//!   counted;
//! * the name's declared code-unit count must fit inside its own record
//!   before the string is allocated;
//! * decoded pattern bytes are charged against the caller's running total,
//!   the same [`crate::abr::MAX_TOTAL_ALPHA_BYTES`] budget the stamp planes
//!   draw on, so a file cannot spend it twice over. A pattern that would
//!   overshoot is skipped, not fatal.
//!
//! Judgment calls:
//!
//! * The record length is padded up to a four-byte boundary and the padded
//!   region is the record (the reference's `read_pattern` does the same).
//!   Trailing pad bytes are simply never read.
//! * A zero-length record is treated as section padding: parsing stops and
//!   nothing is counted (advancing by zero would spin). A short tail
//!   (fewer than four bytes) is padding too.
//! * The channel's depth is read twice (a `u32` and a `u16`) and the two
//!   must agree; a mismatch makes the whole record undecodable, never a
//!   partial pattern.
//! * A written slot is read in full even when its declared length is 0;
//!   the sample data is then empty (a raw channel decodes to zeros, an RLE
//!   channel has no row table and the record is skipped). The public PSD
//!   prose says a zero length means "skip", but the reference reads on and
//!   no fixture in the tree settles which is right, so this reader follows
//!   the reference and says so.
//! * Compression 0 is raw and 1 is PackBits with a `u16` row-length table
//!   per row. RLE is decoded for 8-bit channels only: 16-bit and 32-bit RLE
//!   rows make the record undecodable (16-bit RLE is refused for stamps too,
//!   `abr` module docs).
//! * A channel rectangle must start at or after the pattern rectangle's
//!   origin and lie inside it. A real pattern writes each channel over the
//!   full pattern or a sub-rect of it; anything else can only be hostile or
//!   corrupt, and skipping the record is the soft answer.
//! * A short raw channel leaves the missing samples at 0 rather than
//!   leaving the destination untouched: the RGB plane starts at zero, so the
//!   two are indistinguishable (alpha is already 255 and no channel writes
//!   it).
//! * Only the colour slots are blitted (RGB: written slots 0/1/2;
//!   grayscale and indexed: written slot 0). Later written slots are decoded
//!   and dropped.
//! * The stored `x`/`y` size fields are read and discarded: the real file's
//!   oracle shows them swapped against the rectangle (a 595x597 pattern is
//!   stored as x 597, y 595), so the rectangle is the only sizing truth.
//!
//! Non-goals (v1): writing `.abr` (never — the format is read-only for
//! umber), naming the stored blend-mode code (`BlnM.CBrn` stays as stored;
//! mapping it is 3.1b/3.1c's call), and sampling the pattern in the paint
//! engine (kill 3.1c). Carrying the pattern does not change a painted pixel.

use crate::abr::{decode_packbits_row, malformed, AbrError, Reader, MAX_TOTAL_ALPHA_BYTES};

/// Maximum accepted pattern width and height, in pixels (per side).
///
/// The same 4096 the stamp reader uses. Patterns can be larger than tips in
/// the wild, but a pattern that big is already a megabyte of RGBA and the
/// texture is tiled across the canvas, so 4096 is the v1 boundary.
pub const MAX_PATTERN_DIMENSION: u32 = 4096;

/// Maximum number of patterns decoded from one file.
///
/// Pattern packs routinely share one texture across dozens of brushes, so
/// the cap counts *records*, not references. Records past the cap are
/// skipped and counted in [`crate::abr::AbrFile::skipped_patterns`].
pub const MAX_PATTERNS: usize = 64;

/// One decoded `patt` pattern: an RGBA8 tiling image with no alpha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbrPattern {
    /// The pattern id (a Pascal string decoded as Latin-1) — the link key
    /// a brush names through `Txtr.Idnt`.
    pub id: String,
    /// The pattern's display name (UTF-16, trailing NUL dropped).
    pub name: String,
    /// Pattern width in pixels.
    pub width: u32,
    /// Pattern height in pixels.
    pub height: u32,
    /// RGBA8, row-major, exactly `width * height * 4` bytes. RGB is the
    /// pattern's colour; alpha is 255 everywhere.
    pub rgba: Vec<u8>,
}

/// The pattern's colour mode (the record's stored image-mode field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Mode 1: one gray channel, replicated to R/G/B.
    Grayscale,
    /// Mode 2: one 8-bit palette index per pixel, 256-entry RGB palette.
    Indexed,
    /// Mode 3: three channels, R/G/B.
    Rgb,
}

/// One channel's destination rectangle inside the pattern.
#[derive(Debug, Clone, Copy)]
struct ChannelRect {
    /// Left offset inside the pattern (pixels).
    x: usize,
    /// Top offset inside the pattern (pixels).
    y: usize,
    /// Channel width in pixels (never zero).
    width: usize,
    /// Channel height in pixels (never zero).
    height: usize,
}

/// Decodes every pattern record of one `patt` section payload.
///
/// Soft by design: an undecodable record is skipped and counted, and the
/// function never fails. `decoded_bytes` is the file-wide running total of
/// decoded planes (shared with the `samp` reader); accepted patterns are
/// charged to it, and one that would exceed
/// [`crate::abr::MAX_TOTAL_ALPHA_BYTES`] is skipped.
pub(crate) fn parse_patterns(
    payload: &[u8],
    patterns: &mut Vec<AbrPattern>,
    skipped: &mut u32,
    decoded_bytes: &mut usize,
) {
    parse_patterns_into(
        payload,
        patterns,
        skipped,
        decoded_bytes,
        MAX_TOTAL_ALPHA_BYTES,
    );
}

/// [`parse_patterns`] with an explicit shared budget (the test seam for the
/// budget; production always passes [`MAX_TOTAL_ALPHA_BYTES`]).
fn parse_patterns_into(
    payload: &[u8],
    patterns: &mut Vec<AbrPattern>,
    skipped: &mut u32,
    decoded_bytes: &mut usize,
    max_total_bytes: usize,
) {
    let mut reader = Reader::new(payload);
    while reader.remaining() > 0 {
        if reader.remaining() < 4 {
            // A tail too short for a record length cannot be a record;
            // treat it as padding.
            break;
        }
        let Ok(declared_u32) = reader.u32() else {
            break;
        };
        let Ok(declared) = usize::try_from(declared_u32) else {
            break;
        };
        if declared == 0 {
            // No record; advancing by zero would spin, and a zero length
            // at the tail is section padding.
            break;
        }
        let pad = (4 - (declared % 4)) % 4;
        let Some(padded) = declared.checked_add(pad) else {
            break;
        };
        if padded > reader.remaining() {
            // The record runs past the section: count it once and stop.
            *skipped = skipped.saturating_add(1);
            break;
        }
        let Ok(body) = reader.take(padded) else {
            break;
        };

        if patterns.len() >= MAX_PATTERNS {
            // The per-file cap: skip by length, before any decode work.
            *skipped = skipped.saturating_add(1);
            continue;
        }
        match decode_pattern(body, decoded_bytes, max_total_bytes) {
            Ok(pattern) => patterns.push(pattern),
            Err(_) => *skipped = skipped.saturating_add(1),
        }
    }
}

/// Decodes one pattern record body (the padded bytes after its length
/// prefix).
fn decode_pattern(
    body: &[u8],
    decoded_bytes: &mut usize,
    max_total_bytes: usize,
) -> Result<AbrPattern, AbrError> {
    let mut reader = Reader::new(body);

    let version = reader.u32()?;
    if version != 1 {
        return Err(malformed("unsupported pattern record version"));
    }
    let mode = match reader.u32()? {
        1 => Mode::Grayscale,
        2 => Mode::Indexed,
        3 => Mode::Rgb,
        _ => return Err(malformed("unsupported pattern colour mode")),
    };

    // (vertical, horizontal) size, redundant with the rectangle below.
    let _vertical = reader.i16()?;
    let _horizontal = reader.i16()?;

    let name = read_name(&mut reader)?;
    let id = read_pascal_id(&mut reader)?;

    let palette = if mode == Mode::Indexed {
        let mut entries = [[0u8; 3]; 256];
        for entry in &mut entries {
            let rgb = reader.take(3)?;
            entry.copy_from_slice(rgb);
        }
        reader.skip(4)?;
        Some(entries)
    } else {
        None
    };

    if reader.u32()? != 3 {
        return Err(malformed("unsupported pattern pixel-array version"));
    }
    let _array_length = reader.u32()?; // read and ignored

    let top = reader.u32()?;
    let left = reader.u32()?;
    let bottom = reader.u32()?;
    let right = reader.u32()?;
    let width = right
        .checked_sub(left)
        .ok_or_else(|| malformed("pattern rectangle is inverted"))?;
    let height = bottom
        .checked_sub(top)
        .ok_or_else(|| malformed("pattern rectangle is inverted"))?;
    if width == 0 || height == 0 {
        return Err(malformed("pattern rectangle has no area"));
    }
    if width > MAX_PATTERN_DIMENSION || height > MAX_PATTERN_DIMENSION {
        // Checked before the plane is sized: a 4096x4096 cap keeps one
        // hostile rectangle from asking for gigabytes.
        return Err(malformed("pattern rectangle exceeds the size cap"));
    }

    let width_px =
        usize::try_from(width).map_err(|_| malformed("pattern width does not fit in memory"))?;
    let height_px =
        usize::try_from(height).map_err(|_| malformed("pattern height does not fit in memory"))?;
    let pixel_count = width_px
        .checked_mul(height_px)
        .ok_or_else(|| malformed("pattern size overflow"))?;
    let byte_count = pixel_count
        .checked_mul(4)
        .ok_or_else(|| malformed("pattern size overflow"))?;
    if byte_count > max_total_bytes.saturating_sub(*decoded_bytes) {
        return Err(malformed(
            "pattern exceeds the per-file decoded-byte budget",
        ));
    }
    let mut rgba = vec![0u8; byte_count];
    for alpha in rgba.iter_mut().skip(3).step_by(4) {
        *alpha = 255;
    }

    // The slot loop runs `count + 2`: the two extras are the mask slots of
    // the public layout (usually written out with a zero flag).
    let slot_count = reader.u32()?.saturating_add(2);
    let mut written = 0usize;
    for _ in 0..slot_count {
        if reader.u32()? == 0 {
            // An empty slot: only its flag is present.
            continue;
        }
        let length = usize::try_from(reader.u32()?)
            .map_err(|_| malformed("pattern channel length does not fit in memory"))?;
        let depth = reader.u32()?;
        let channel_top = reader.u32()?;
        let channel_left = reader.u32()?;
        let channel_bottom = reader.u32()?;
        let channel_right = reader.u32()?;
        let depth_again = reader.u16()?;
        let compression = reader.u8()?;

        if u32::from(depth_again) != depth {
            return Err(malformed("pattern channel depth mismatch"));
        }
        let sample_bytes = match depth {
            8 => 1,
            16 => 2,
            32 => 4,
            _ => return Err(malformed("unsupported pattern channel depth")),
        };
        if mode == Mode::Indexed && depth != 8 {
            return Err(malformed("indexed pattern channels are 8-bit only"));
        }
        if mode == Mode::Indexed && compression == 1 {
            // A palette index has no RLE representation this reader decodes
            // (the reference refuses it too).
            return Err(malformed("indexed pattern channels cannot be RLE"));
        }

        // `length` covers the depth, the rectangle, the repeated depth and
        // the compression byte (4 + 16 + 2 + 1 = 23) before the samples.
        let channel_data = reader.take(length.saturating_sub(23))?;

        let channel_width = channel_right
            .checked_sub(channel_left)
            .ok_or_else(|| malformed("pattern channel rectangle is inverted"))?;
        let channel_height = channel_bottom
            .checked_sub(channel_top)
            .ok_or_else(|| malformed("pattern channel rectangle is inverted"))?;
        if channel_width == 0 || channel_height == 0 {
            return Err(malformed("pattern channel rectangle has no area"));
        }
        let offset_x = channel_left
            .checked_sub(left)
            .ok_or_else(|| malformed("pattern channel starts outside the pattern"))?;
        let offset_y = channel_top
            .checked_sub(top)
            .ok_or_else(|| malformed("pattern channel starts outside the pattern"))?;
        if offset_x.saturating_add(channel_width) > width
            || offset_y.saturating_add(channel_height) > height
        {
            return Err(malformed("pattern channel extends past the pattern"));
        }

        let rect = ChannelRect {
            x: usize::try_from(offset_x)
                .map_err(|_| malformed("pattern channel offset out of range"))?,
            y: usize::try_from(offset_y)
                .map_err(|_| malformed("pattern channel offset out of range"))?,
            width: usize::try_from(channel_width)
                .map_err(|_| malformed("pattern channel width out of range"))?,
            height: usize::try_from(channel_height)
                .map_err(|_| malformed("pattern channel height out of range"))?,
        };

        let pixels = decode_channel(channel_data, rect, sample_bytes, depth, compression)?;
        let blit = match mode {
            Mode::Rgb => written < 3,
            Mode::Grayscale | Mode::Indexed => written == 0,
        };
        if blit {
            blit_channel(
                &mut rgba,
                width_px,
                &pixels,
                rect,
                mode,
                written,
                palette.as_ref(),
            );
        }
        written += 1;
    }

    // Charged only for a pattern that is published; a skipped record leaves
    // the budget untouched.
    *decoded_bytes = decoded_bytes.saturating_add(byte_count);

    Ok(AbrPattern {
        id,
        name,
        width,
        height,
        rgba,
    })
}

/// Reads the UTF-16BE name: a `u32` code-unit count, then the units, with a
/// trailing NUL unit dropped.
///
/// The declared count must fit inside the record before anything is
/// allocated. Decoding is lossy (an unpaired surrogate becomes U+FFFD),
/// matching the descriptor reader's text policy: text content never fails.
fn read_name(reader: &mut Reader<'_>) -> Result<String, AbrError> {
    let units = usize::try_from(reader.u32()?)
        .map_err(|_| malformed("pattern name length does not fit in memory"))?;
    let bytes = units
        .checked_mul(2)
        .ok_or_else(|| malformed("pattern name length overflow"))?;
    if bytes > reader.remaining() {
        return Err(malformed("pattern name runs past its record"));
    }
    let raw = reader.take(bytes)?;
    let mut wide: Vec<u16> = raw
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect();
    if wide.last() == Some(&0) {
        wide.pop();
    }
    Ok(String::from_utf16_lossy(&wide))
}

/// Reads the Pascal-string id: a one-byte length, then Latin-1 bytes (each
/// byte is the code point of the same value, like the `samp` ids).
fn read_pascal_id(reader: &mut Reader<'_>) -> Result<String, AbrError> {
    let len = usize::from(reader.u8()?);
    let bytes = reader.take(len)?;
    Ok(bytes.iter().map(|&byte| byte as char).collect())
}

/// Decodes one channel's samples into `rect.width * rect.height` 8-bit
/// values, in channel order.
fn decode_channel(
    data: &[u8],
    rect: ChannelRect,
    sample_bytes: usize,
    depth: u32,
    compression: u8,
) -> Result<Vec<u8>, AbrError> {
    if rect.width == 0 || rect.height == 0 {
        return Err(malformed("pattern channel has no area"));
    }
    let pixel_count = rect
        .width
        .checked_mul(rect.height)
        .ok_or_else(|| malformed("pattern channel size overflow"))?;
    let mut pixels = vec![0u8; pixel_count];
    match compression {
        0 => {
            for (index, pixel) in pixels.iter_mut().enumerate() {
                let start = index.saturating_mul(sample_bytes);
                if let Some(sample) = data.get(start..start.saturating_add(sample_bytes)) {
                    *pixel = sample_to_u8(sample, depth);
                }
            }
        }
        1 => {
            if sample_bytes != 1 {
                return Err(malformed("RLE pattern channels are 8-bit only"));
            }
            let mut rows = Reader::new(data);
            let mut lengths = Vec::with_capacity(rect.height);
            for _ in 0..rect.height {
                lengths.push(usize::from(rows.u16()?));
            }
            for (row, &encoded_len) in pixels.chunks_exact_mut(rect.width).zip(lengths.iter()) {
                let encoded = rows.take(encoded_len)?;
                decode_packbits_row(encoded, row)?;
            }
        }
        _ => return Err(malformed("unsupported pattern channel compression")),
    }
    Ok(pixels)
}

/// Blits one decoded channel into the RGBA pattern at `rect`.
fn blit_channel(
    target: &mut [u8],
    target_width: usize,
    pixels: &[u8],
    rect: ChannelRect,
    mode: Mode,
    channel: usize,
    palette: Option<&[[u8; 3]; 256]>,
) {
    for y in 0..rect.height {
        for x in 0..rect.width {
            let source = pixels
                .get(y.saturating_mul(rect.width).saturating_add(x))
                .copied()
                .unwrap_or(0);
            let Some(dst) = (rect.y + y)
                .checked_mul(target_width)
                .and_then(|row| row.checked_add(rect.x + x))
                .and_then(|pixel| pixel.checked_mul(4))
            else {
                continue;
            };
            match mode {
                Mode::Rgb => {
                    if let Some(slot) = dst.checked_add(channel).and_then(|at| target.get_mut(at)) {
                        *slot = source;
                    }
                }
                Mode::Grayscale => {
                    if let Some(slot) = target.get_mut(dst..dst.saturating_add(3)) {
                        slot.fill(source);
                    }
                }
                Mode::Indexed => {
                    if let Some(entry) =
                        palette.and_then(|entries| entries.get(usize::from(source)))
                    {
                        if let Some(slot) = target.get_mut(dst..dst.saturating_add(3)) {
                            slot.copy_from_slice(entry);
                        }
                    }
                }
            }
        }
    }
}

/// Narrows one big-endian channel sample to 8 bits: 8- and 16-bit keep the
/// most significant byte, 32-bit is a float clamped to `0.0..=1.0` and
/// scaled by 255 (the reference's `sample_to_u8` shape). An unsupported
/// depth yields 0; callers reject such depths before decoding.
fn sample_to_u8(sample: &[u8], depth: u32) -> u8 {
    match depth {
        8 | 16 => sample.first().copied().unwrap_or(0),
        32 => sample
            .get(..4)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map_or(0, |bytes| float_to_u8(f32::from_be_bytes(bytes))),
        _ => 0,
    }
}

/// Clamps a float sample to `0.0..=1.0` and scales it to 8 bits. NaN becomes
/// 0 through `max` then `min`; `clamp` would propagate it — hence the lint
/// allow, whose rewrite would change behaviour.
#[allow(clippy::manual_clamp)]
fn float_to_u8(value: f32) -> u8 {
    (value.max(0.0).min(1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Big-endian writer for synthetic `patt` records.
    struct Record {
        bytes: Vec<u8>,
    }

    impl Record {
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

        fn raw(&mut self, bytes: &[u8]) {
            self.bytes.extend_from_slice(bytes);
        }

        fn unicode(&mut self, text: &str) {
            let units: Vec<u16> = text.encode_utf16().collect();
            self.u32(u32::try_from(units.len()).expect("test text fits a u32"));
            for unit in units {
                self.u16(unit);
            }
        }
    }

    /// One channel slot of a synthetic record.
    struct ChannelSpec<'a> {
        written: bool,
        depth: u32,
        depth_again: u16,
        compression: u8,
        top: u32,
        left: u32,
        bottom: u32,
        right: u32,
        data: &'a [u8],
        length: u32,
    }

    impl<'a> ChannelSpec<'a> {
        /// A written raw channel of `right` x `bottom` at the pattern
        /// origin, with the length field `23 + data.len()`.
        fn raw(depth: u32, right: u32, bottom: u32, data: &'a [u8]) -> Self {
            Self {
                written: true,
                depth,
                depth_again: u16::try_from(depth).expect("test depth fits a u16"),
                compression: 0,
                top: 0,
                left: 0,
                bottom,
                right,
                data,
                length: 23 + u32::try_from(data.len()).expect("test data fits a u32"),
            }
        }

        /// A written RLE channel of `right` x `bottom` at the pattern
        /// origin.
        fn rle(right: u32, bottom: u32, data: &'a [u8]) -> Self {
            Self {
                compression: 1,
                ..Self::raw(8, right, bottom, data)
            }
        }

        /// An empty slot: only its written flag is encoded.
        fn empty() -> Self {
            Self {
                written: false,
                ..Self::raw(8, 1, 1, &[])
            }
        }
    }

    /// The mutable parts of one synthetic pattern record.
    struct PatternSpec<'a> {
        version: u32,
        mode: u32,
        name: &'a str,
        id: &'a [u8],
        palette: [[u8; 3]; 256],
        array_version: u32,
        top: u32,
        left: u32,
        bottom: u32,
        right: u32,
        channel_count: u32,
        channels: Vec<ChannelSpec<'a>>,
    }

    impl<'a> PatternSpec<'a> {
        /// A record of `width` x `height` in `mode` (1/2/3) with no
        /// channels yet.
        fn new(mode: u32, name: &'a str, id: &'a [u8], width: u32, height: u32) -> Self {
            Self {
                version: 1,
                mode,
                name,
                id,
                palette: [[0u8; 3]; 256],
                array_version: 3,
                top: 0,
                left: 0,
                bottom: height,
                right: width,
                channel_count: match mode {
                    3 => 3,
                    _ => 1,
                },
                channels: Vec::new(),
            }
        }

        fn channel(&mut self, channel: ChannelSpec<'a>) -> &mut Self {
            self.channels.push(channel);
            self
        }

        /// Encodes the record: length prefix, body, zero padding to a
        /// four-byte boundary.
        fn encode(&self) -> Vec<u8> {
            let body = self.body();
            let mut record = Record::new();
            record.u32(u32::try_from(body.len()).expect("test record fits a u32"));
            record.raw(&body);
            let mut bytes = record.bytes;
            while !bytes.len().is_multiple_of(4) {
                bytes.push(0);
            }
            bytes
        }

        /// The record body (everything after the length prefix).
        fn body(&self) -> Vec<u8> {
            let mut body = Record::new();
            body.u32(self.version);
            body.u32(self.mode);
            body.i16(0); // vertical size (unused)
            body.i16(0); // horizontal size (unused)
            body.unicode(self.name);
            body.u8(u8::try_from(self.id.len()).expect("test id fits a u8"));
            body.raw(self.id);
            if self.mode == 2 {
                for entry in &self.palette {
                    body.raw(entry);
                }
                body.raw(&[0, 0, 0, 0]);
            }
            body.u32(self.array_version);
            body.u32(0); // pixel-array length (ignored)
            body.u32(self.top);
            body.u32(self.left);
            body.u32(self.bottom);
            body.u32(self.right);
            body.u32(self.channel_count);
            for slot in &self.channels {
                if !slot.written {
                    body.u32(0);
                    continue;
                }
                body.u32(1); // written
                body.u32(slot.length);
                body.u32(slot.depth);
                body.u32(slot.top);
                body.u32(slot.left);
                body.u32(slot.bottom);
                body.u32(slot.right);
                body.u16(slot.depth_again);
                body.u8(slot.compression);
                body.raw(slot.data);
            }
            // The slot loop runs channel_count + 2: the two mask slots are
            // usually written out with an empty flag.
            let slots = self.channels.len();
            let expected = usize::try_from(self.channel_count)
                .expect("test channel count fits a usize")
                .saturating_add(2);
            for _ in slots..expected {
                body.u32(0);
            }
            body.bytes
        }
    }

    /// Decodes one record with the production budget. `record` is a full
    /// length-prefixed encoding (as the section loop receives it), so the
    /// 4-byte length prefix is stripped before the body decode.
    fn decode(record: &[u8]) -> Result<AbrPattern, AbrError> {
        let mut total = 0usize;
        decode_pattern(
            record.get(4..).unwrap_or(record),
            &mut total,
            MAX_TOTAL_ALPHA_BYTES,
        )
    }

    /// Runs the section loop over `records` and returns the outcome.
    fn parse(records: &[Vec<u8>]) -> (Vec<AbrPattern>, u32) {
        parse_with_budget(records, MAX_TOTAL_ALPHA_BYTES)
    }

    fn parse_with_budget(records: &[Vec<u8>], max_total_bytes: usize) -> (Vec<AbrPattern>, u32) {
        let payload: Vec<u8> = records.iter().flatten().copied().collect();
        let mut patterns = Vec::new();
        let mut skipped = 0u32;
        let mut total = 0usize;
        parse_patterns_into(
            &payload,
            &mut patterns,
            &mut skipped,
            &mut total,
            max_total_bytes,
        );
        (patterns, skipped)
    }

    /// Pin 1: grayscale 2x2 raw maps to white-balanced gray RGBA, and the
    /// slot length field is exactly 23 + 4.
    #[test]
    fn grayscale_raw_maps_to_rgba() {
        let mut spec = PatternSpec::new(1, "Gray", b"p1", 2, 2);
        let mut channel = ChannelSpec::raw(8, 2, 2, &[10, 20, 30, 40]);
        channel.length = 27; // 23 overhead + 4 samples, pinned explicitly
        spec.channel(channel);
        let pattern = decode(&spec.encode()).expect("grayscale record decodes");
        assert_eq!(pattern.width, 2);
        assert_eq!(pattern.height, 2);
        assert_eq!(
            pattern.rgba,
            vec![10, 10, 10, 255, 20, 20, 20, 255, 30, 30, 30, 255, 40, 40, 40, 255,]
        );
    }

    /// Pin 2: RGB 2x1 raw maps written slots 0/1/2 to R/G/B per pixel.
    #[test]
    fn rgb_raw_maps_written_slots_to_channels() {
        let mut spec = PatternSpec::new(3, "Rgb", b"p2", 2, 1);
        spec.channel(ChannelSpec::raw(8, 2, 1, &[1, 2]))
            .channel(ChannelSpec::raw(8, 2, 1, &[3, 4]))
            .channel(ChannelSpec::raw(8, 2, 1, &[5, 6]));
        let pattern = decode(&spec.encode()).expect("rgb record decodes");
        assert_eq!(
            pattern.rgba,
            vec![1, 3, 5, 255, 2, 4, 6, 255],
            "pixel 0 is (R1,G3,B5), pixel 1 is (R2,G4,B6)"
        );
    }

    /// Pin 3: indexed 2x1 resolves through the 256-entry palette, which
    /// also proves the 768 + 4 palette skip.
    #[test]
    fn indexed_raw_resolves_palette() {
        let mut spec = PatternSpec::new(2, "Idx", b"p3", 2, 1);
        spec.palette[7] = [9, 8, 7];
        spec.palette[0] = [1, 2, 3];
        spec.channel(ChannelSpec::raw(8, 2, 1, &[7, 0]));
        let pattern = decode(&spec.encode()).expect("indexed record decodes");
        assert_eq!(pattern.rgba, vec![9, 8, 7, 255, 1, 2, 3, 255]);
    }

    /// Pin 4: a grayscale 3x1 RLE row decodes through PackBits (header
    /// 0xFE repeats three times).
    #[test]
    fn grayscale_rle_row_decodes() {
        let data = [0x00, 0x02, 0xFE, 0x55]; // one row of two packed bytes
        let mut spec = PatternSpec::new(1, "Rle", b"p4", 3, 1);
        spec.channel(ChannelSpec::rle(3, 1, &data));
        let pattern = decode(&spec.encode()).expect("RLE record decodes");
        assert_eq!(
            pattern.rgba,
            vec![0x55, 0x55, 0x55, 255, 0x55, 0x55, 0x55, 255, 0x55, 0x55, 0x55, 255]
        );
    }

    /// Pin 5: a 16-bit raw sample keeps its high byte.
    #[test]
    fn sixteen_bit_sample_keeps_the_high_byte() {
        let mut spec = PatternSpec::new(1, "Deep", b"p5", 1, 1);
        spec.channel(ChannelSpec::raw(16, 1, 1, &[0xAB, 0xCD]));
        let pattern = decode(&spec.encode()).expect("16-bit record decodes");
        assert_eq!(pattern.rgba, vec![0xAB, 0xAB, 0xAB, 255]);
    }

    /// Pin 6: a channel sub-rect lands at its pattern offset; the rest of
    /// the pattern stays black.
    #[test]
    fn channel_sub_rect_lands_at_its_offset() {
        let mut spec = PatternSpec::new(1, "Sub", b"p6", 2, 2);
        spec.channel(ChannelSpec {
            top: 1,
            left: 1,
            bottom: 2,
            right: 2,
            ..ChannelSpec::raw(8, 1, 1, &[99])
        });
        let pattern = decode(&spec.encode()).expect("sub-rect record decodes");
        assert_eq!(
            pattern.rgba,
            vec![0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 99, 99, 99, 255,]
        );
    }

    /// Pin 7: an unwritten slot between written ones does not shift the
    /// channel mapping.
    #[test]
    fn unwritten_slot_between_written_slots_is_ignored() {
        let mut spec = PatternSpec::new(3, "Gap", b"p7", 2, 1);
        spec.channel(ChannelSpec::raw(8, 2, 1, &[1, 2]))
            .channel(ChannelSpec::empty())
            .channel(ChannelSpec::raw(8, 2, 1, &[3, 4]))
            .channel(ChannelSpec::raw(8, 2, 1, &[5, 6]));
        let pattern = decode(&spec.encode()).expect("gapped record decodes");
        assert_eq!(pattern.rgba, vec![1, 3, 5, 255, 2, 4, 6, 255]);
    }

    /// Pin 8: a trailing NUL unit is dropped, non-ASCII text round-trips,
    /// and the id decodes as Latin-1.
    #[test]
    fn name_and_id_decode_as_the_reference_does() {
        let mut spec = PatternSpec::new(1, "Caf\u{e9} Paint\0", &[0x41, 0xE9], 1, 1);
        spec.channel(ChannelSpec::raw(8, 1, 1, &[7]));
        let pattern = decode(&spec.encode()).expect("text record decodes");
        assert_eq!(pattern.name, "Caf\u{e9} Paint");
        assert_eq!(pattern.id, "A\u{e9}");
        assert_eq!(pattern.rgba, vec![7, 7, 7, 255]);
    }

    /// Pin 9: two records in one section, the first needing padding, are
    /// both found in file order.
    #[test]
    fn two_padded_records_are_both_found_in_order() {
        // These bodies are 90 and 91 bytes, so the first record carries two
        // pad bytes and the second one.
        let mut first = PatternSpec::new(1, "On", b"a", 1, 1);
        first.channel(ChannelSpec::raw(8, 1, 1, &[1]));
        let mut second = PatternSpec::new(1, "Tw", b"bb", 1, 1);
        second.channel(ChannelSpec::raw(8, 1, 1, &[2]));
        assert_eq!(first.body().len(), 90, "the first body needs 2 pad bytes");
        assert_eq!(second.body().len(), 91, "the second body needs 1 pad byte");
        let (patterns, skipped) = parse(&[first.encode(), second.encode()]);
        assert_eq!(skipped, 0);
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0].name, "On");
        assert_eq!(patterns[1].name, "Tw");
        assert_eq!(patterns[0].rgba, vec![1, 1, 1, 255]);
        assert_eq!(patterns[1].rgba, vec![2, 2, 2, 255]);
    }

    /// Pin 10: each undecodable shape is skipped (and only skipped).
    ///
    /// All seven shapes go through the same variant-agnostic arm of the
    /// section loop (`Err(_) => skipped += 1`), so one of them is re-run at
    /// walk level in `abr.rs` (`a_bad_pattern_record_does_not_break_the_file`)
    /// where samples and a following `desc` must still parse.
    #[test]
    fn undecodable_records_are_skipped_not_fatal() {
        let base = || {
            let mut spec = PatternSpec::new(1, "Bad", b"p10", 1, 1);
            spec.channel(ChannelSpec::raw(8, 1, 1, &[9]));
            spec
        };

        // mode 4
        let mut mode = base();
        mode.mode = 4;
        // record version 2
        let mut version = base();
        version.version = 2;
        // pixel-array version 2
        let mut array_version = base();
        array_version.array_version = 2;
        // depth mismatch: u32 8, u16 16
        let mut depth_mismatch = base();
        depth_mismatch.channels[0].depth_again = 16;
        // depth 1
        let mut depth_one = base();
        depth_one.channels[0].depth = 1;
        depth_one.channels[0].depth_again = 1;
        // compression 2
        let mut compression_two = base();
        compression_two.channels[0].compression = 2;
        // indexed + RLE
        let mut indexed_rle = PatternSpec::new(2, "IdxRle", b"p10", 1, 1);
        indexed_rle.channel(ChannelSpec::rle(1, 1, &[0x00, 0x01, 0x00, 0x00]));

        for (label, spec) in [
            ("mode 4", mode),
            ("version 2", version),
            ("array version 2", array_version),
            ("depth mismatch", depth_mismatch),
            ("depth 1", depth_one),
            ("compression 2", compression_two),
            ("indexed + RLE", indexed_rle),
        ] {
            let record = spec.encode();
            assert!(
                decode(&record).is_err(),
                "{label}: the decoder must refuse the record"
            );
            let (patterns, skipped) = parse(&[record]);
            assert!(patterns.is_empty(), "{label}: no pattern published");
            assert_eq!(skipped, 1, "{label}: counted once as skipped");
        }
    }

    /// Pin 11: a record whose declared length runs past the section end is
    /// counted once and stops pattern parsing.
    #[test]
    fn a_record_past_the_section_end_is_counted_once() {
        let mut payload = Record::new();
        payload.u32(1000); // declared length far past the payload
        payload.u32(1); // a plausible version, never reached
        let mut patterns = Vec::new();
        let mut skipped = 0u32;
        let mut total = 0usize;
        parse_patterns_into(
            &payload.bytes,
            &mut patterns,
            &mut skipped,
            &mut total,
            MAX_TOTAL_ALPHA_BYTES,
        );
        assert!(patterns.is_empty());
        assert_eq!(skipped, 1);
        assert_eq!(total, 0, "a skipped record is not charged to the budget");
    }

    /// Pin 12: a rectangle past the per-side cap is refused before the
    /// plane is sized (4097 x u32::MAX would otherwise be ~64 GiB).
    #[test]
    fn oversized_rectangle_is_refused_before_allocating() {
        let spec = PatternSpec::new(1, "Huge", b"p12", MAX_PATTERN_DIMENSION + 1, u32::MAX);
        let error = decode(&spec.encode()).expect_err("the cap fires first");
        assert!(
            matches!(error, AbrError::Malformed { .. }),
            "expected Malformed, got {error:?}"
        );
        let (patterns, skipped) = parse(&[spec.encode()]);
        assert!(patterns.is_empty());
        assert_eq!(skipped, 1);
    }

    /// A name whose declared unit count does not fit inside its record is
    /// skipped before the string is allocated.
    #[test]
    fn a_name_longer_than_its_record_is_skipped() {
        let mut body = Record::new();
        body.u32(1); // version
        body.u32(1); // grayscale
        body.i16(0);
        body.i16(0);
        body.u32(1_000_000); // code units claimed, none present
        let mut payload = Record::new();
        payload.u32(u32::try_from(body.bytes.len()).expect("test body fits a u32"));
        payload.raw(&body.bytes);
        let (patterns, skipped) = parse(&[payload.bytes]);
        assert!(patterns.is_empty());
        assert_eq!(skipped, 1);
    }

    /// The per-file pattern cap skips the record past it and counts it.
    #[test]
    fn patterns_past_the_cap_are_skipped_and_counted() {
        let records: Vec<Vec<u8>> = (0..=MAX_PATTERNS)
            .map(|index| {
                let mut spec = PatternSpec::new(1, "Cap", b"p", 1, 1);
                let value = u8::try_from(index % 256).expect("the byte fits");
                let data = [value];
                spec.channel(ChannelSpec::raw(8, 1, 1, &data));
                spec.encode()
            })
            .collect();
        let (patterns, skipped) = parse(&records);
        assert_eq!(patterns.len(), MAX_PATTERNS);
        assert_eq!(skipped, 1);
    }

    /// Decoded bytes are charged against the shared budget: the first
    /// pattern fits, the second overshoots and is skipped.
    #[test]
    fn decoded_bytes_share_the_stamp_budget() {
        let mut first = PatternSpec::new(1, "A", b"b1", 1, 1);
        first.channel(ChannelSpec::raw(8, 1, 1, &[1]));
        let mut second = PatternSpec::new(1, "B", b"b2", 1, 1);
        second.channel(ChannelSpec::raw(8, 1, 1, &[2]));
        // Each 1x1 pattern decodes to 4 RGBA bytes.
        let (patterns, skipped) = parse_with_budget(&[first.encode(), second.encode()], 5);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].name, "A");
        assert_eq!(skipped, 1);
    }

    /// A zero-length tail is padding, not a record: nothing is counted.
    #[test]
    fn a_zero_length_tail_is_padding() {
        let mut payload = Record::new();
        payload.u32(0);
        payload.u32(0);
        let (patterns, skipped) = parse(&[payload.bytes]);
        assert!(patterns.is_empty());
        assert_eq!(skipped, 0);
    }
}
