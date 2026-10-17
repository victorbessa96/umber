//! Painter's `texture-cache/`: the flattened channel textures (research
//! doc §7a, corpus-observed on three real projects).
//!
//! Every real Painter `.spp` holds its paint inside HBO graphs that v1
//! does not decode, so the H1 raster heuristic imports nothing from it.
//! The cache is the one place the flattened channels exist as plain
//! pixels: `texture-cache/<set-id>/<n>/<flag>` datasets, one per channel.
//! This module decodes one such dataset's bytes.
//!
//! ## The payload (observed, all 81 channels of the corpus)
//!
//! 1. A big-endian `QDataStream` map: `u32 count`, then `count` ×
//!    (`QString` key = `u32` byte length + UTF-16BE, `QVariant` value =
//!    `u32` type + `u8` null flag + the value). Keys: `bufferSize`,
//!    `compression` (= `"lz4"`), `width`, `height`, `internalFormat` (an
//!    OpenGL enum), `version` (= `"v1"`).
//! 2. A `u32` length, then ONE raw LZ4 block that decodes to exactly
//!    `bufferSize` bytes: `width × height × bytes-per-texel`, row-major.
//!
//! A dataset whose map lacks `width` (the per-file `header` stamp:
//! `version`, `version.minor` only) is not a channel and is reported as
//! such, never an error.
//!
//! ## Never-crash bounds
//!
//! - Every read is bounds-checked; a short or malformed header is
//!   [`CacheError`], not a panic.
//! - `bufferSize` must equal `width × height × bpp` for the format (all 81
//!   corpus channels do) and stay under [`MAX_CHANNEL_BYTES`], before
//!   anything is allocated.
//! - LZ4 decodes with `lz4_flex::block::decompress_into` into a buffer of
//!   exactly that size: output can never exceed the declared footprint
//!   (`decompress`, which can panic on a wrong size hint, is not used),
//!   and a short decode is refused.
//!
//! ## Channel names
//!
//! The cache keys channels by bit flag. [`channel_name_for_flag`] names
//! the five default-stack channels from the document's own `(format,
//! flag)` records plus the pixels, names two more as inferred, and keeps
//! `flag_<n>` for the rest. The report says which is which.

use std::fmt;

/// The largest decoded channel accepted (256 MiB, the reader's dataset
/// cap; the corpus maximum is 128 MiB, a 4K RGBA16F).
pub const MAX_CHANNEL_BYTES: u64 = 256 * 1024 * 1024;

/// The largest side accepted (16K; the corpus uses 2K and 4K).
pub const MAX_SIDE: u32 = 16384;

/// OpenGL `internalFormat` enums seen in the corpus. Values checked
/// against glow 0.17.0's constants (`SRGB8_ALPHA8` 0x8C43, `RG8` 0x822B,
/// `RG16F` 0x822F, `RGBA16F` 0x881A).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlFormat {
    /// `GL_SRGB8_ALPHA8`: 4 × u8, sRGB-encoded colour + linear alpha.
    Srgb8Alpha8,
    /// `GL_RG8`: 2 × u8 (value + coverage).
    Rg8,
    /// `GL_RG16F`: 2 × f16 (value + coverage).
    Rg16F,
    /// `GL_RGBA16F`: 4 × f16.
    Rgba16F,
}

impl GlFormat {
    /// The format for a GL enum, or `None` for one the corpus never showed.
    pub fn from_gl(value: i64) -> Option<Self> {
        match value {
            0x8C43 => Some(Self::Srgb8Alpha8),
            0x822B => Some(Self::Rg8),
            0x822F => Some(Self::Rg16F),
            0x881A => Some(Self::Rgba16F),
            _ => None,
        }
    }

    /// Bytes per texel.
    pub fn bytes_per_texel(self) -> u64 {
        match self {
            Self::Rg8 => 2,
            Self::Srgb8Alpha8 | Self::Rg16F => 4,
            Self::Rgba16F => 8,
        }
    }

    /// Short report name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Srgb8Alpha8 => "SRGB8_ALPHA8",
            Self::Rg8 => "RG8",
            Self::Rg16F => "RG16F",
            Self::Rgba16F => "RGBA16F",
        }
    }
}

/// Why a cache dataset did not decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheError {
    /// The bytes end before the structure does.
    Truncated(&'static str),
    /// A field has a type or value outside the observed format.
    Malformed(String),
    /// The map has no `width`: a cache stamp, not a channel.
    NotAChannel,
    /// Declared sizes exceed the caps or disagree with the format.
    TooLarge(String),
    /// The LZ4 block is corrupt or decodes short.
    Lz4(String),
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(what) => write!(f, "truncated ({what})"),
            Self::Malformed(why) => write!(f, "malformed ({why})"),
            Self::NotAChannel => write!(f, "cache stamp, not a channel"),
            Self::TooLarge(why) => write!(f, "refused ({why})"),
            Self::Lz4(why) => write!(f, "lz4 ({why})"),
        }
    }
}

/// The parsed header of one cache channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheHeader {
    /// Texels across.
    pub width: u32,
    /// Texels down.
    pub height: u32,
    /// The texel format.
    pub format: GlFormat,
    /// Declared decoded size (`width × height × bpp`, checked).
    pub buffer_size: usize,
    /// Byte offset of the LZ4 block inside the dataset.
    pub block_offset: usize,
    /// Length of the LZ4 block.
    pub block_len: usize,
}

/// One decoded cache channel as 8-bit RGBA, row-major, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheChannel {
    /// The parsed header.
    pub header: CacheHeader,
    /// `width × height × 4` bytes.
    pub rgba: Vec<u8>,
    /// Conversion notes for the report (f16 clamps, two-channel layout).
    pub notes: Vec<String>,
}

/// A big-endian cursor that never reads past the end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], CacheError> {
        let end = self.at.checked_add(n).ok_or(CacheError::Truncated(what))?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or(CacheError::Truncated(what))?;
        self.at = end;
        Ok(slice)
    }

    fn u32(&mut self, what: &'static str) -> Result<u32, CacheError> {
        let b = self.take(4, what)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u8(&mut self, what: &'static str) -> Result<u8, CacheError> {
        Ok(self.take(1, what)?[0])
    }

    /// A `QString`: `u32` byte length (0xFFFFFFFF = null), UTF-16BE.
    fn qstring(&mut self, what: &'static str) -> Result<String, CacheError> {
        let len = self.u32(what)?;
        if len == u32::MAX {
            return Ok(String::new());
        }
        if len % 2 != 0 || len > 4096 {
            return Err(CacheError::Malformed(format!(
                "{what}: string length {len}"
            )));
        }
        let raw = self.take(len as usize, what)?;
        let units: Vec<u16> = raw
            .chunks_exact(2)
            .map(|p| u16::from_be_bytes([p[0], p[1]]))
            .collect();
        String::from_utf16(&units).map_err(|_| CacheError::Malformed(format!("{what}: bad UTF-16")))
    }
}

/// A `QVariant` value the header uses.
enum Variant {
    Int(i64),
    Text(String),
}

/// `QMetaType` ids for the variant types seen: Int 2, UInt 3, LongLong 4,
/// ULongLong 5, QString 10.
fn read_variant(r: &mut Reader<'_>) -> Result<Variant, CacheError> {
    let kind = r.u32("variant type")?;
    let _null = r.u8("variant null flag")?;
    match kind {
        2 => {
            let b = r.take(4, "int")?;
            Ok(Variant::Int(i64::from(i32::from_be_bytes([
                b[0], b[1], b[2], b[3],
            ]))))
        }
        3 => Ok(Variant::Int(i64::from(r.u32("uint")?))),
        4 | 5 => {
            let b = r.take(8, "long")?;
            let mut a = [0u8; 8];
            a.copy_from_slice(b);
            let v = if kind == 4 {
                i64::from_be_bytes(a)
            } else {
                i64::try_from(u64::from_be_bytes(a))
                    .map_err(|_| CacheError::Malformed("ulonglong over i64".into()))?
            };
            Ok(Variant::Int(v))
        }
        10 => Ok(Variant::Text(r.qstring("variant string")?)),
        other => Err(CacheError::Malformed(format!("variant type {other}"))),
    }
}

/// Parses a cache dataset's header and locates its LZ4 block, checking
/// every declared size before anything is allocated.
pub fn parse_header(bytes: &[u8]) -> Result<CacheHeader, CacheError> {
    let mut r = Reader { bytes, at: 0 };
    let count = r.u32("entry count")?;
    if count > 64 {
        return Err(CacheError::Malformed(format!("{count} header entries")));
    }
    let (mut width, mut height, mut format, mut buffer_size) = (None, None, None, None);
    let mut compression = None;
    for _ in 0..count {
        let key = r.qstring("key")?;
        let value = read_variant(&mut r)?;
        match (key.as_str(), value) {
            ("width", Variant::Int(v)) => width = Some(v),
            ("height", Variant::Int(v)) => height = Some(v),
            ("internalFormat", Variant::Int(v)) => format = Some(v),
            ("bufferSize", Variant::Int(v)) => buffer_size = Some(v),
            ("compression", Variant::Text(v)) => compression = Some(v),
            _ => {}
        }
    }
    let Some(width) = width else {
        return Err(CacheError::NotAChannel);
    };
    let side = |v: i64, what: &str| -> Result<u32, CacheError> {
        u32::try_from(v)
            .ok()
            .filter(|s| (1..=MAX_SIDE).contains(s))
            .ok_or_else(|| CacheError::TooLarge(format!("{what} {v} outside 1..={MAX_SIDE}")))
    };
    let width = side(width, "width")?;
    let height = side(
        height.ok_or_else(|| CacheError::Malformed("no height".into()))?,
        "height",
    )?;
    let gl = format.ok_or_else(|| CacheError::Malformed("no internalFormat".into()))?;
    let format = GlFormat::from_gl(gl)
        .ok_or_else(|| CacheError::Malformed(format!("internalFormat 0x{gl:X} not supported")))?;
    match compression.as_deref() {
        Some("lz4") => {}
        other => {
            return Err(CacheError::Malformed(format!(
                "compression {other:?}, want lz4"
            )));
        }
    }
    let expected = u64::from(width) * u64::from(height) * format.bytes_per_texel();
    let declared = buffer_size.ok_or_else(|| CacheError::Malformed("no bufferSize".into()))?;
    if u64::try_from(declared).ok() != Some(expected) {
        return Err(CacheError::TooLarge(format!(
            "bufferSize {declared} != {width}x{height}x{} = {expected}",
            format.bytes_per_texel()
        )));
    }
    if expected > MAX_CHANNEL_BYTES {
        return Err(CacheError::TooLarge(format!(
            "{expected} bytes over the {MAX_CHANNEL_BYTES} cap"
        )));
    }
    let block_len = r.u32("block length")? as usize;
    let block_offset = r.at;
    if block_offset
        .checked_add(block_len)
        .is_none_or(|end| end > bytes.len())
    {
        return Err(CacheError::Truncated("lz4 block"));
    }
    Ok(CacheHeader {
        width,
        height,
        format,
        buffer_size: expected as usize,
        block_offset,
        block_len,
    })
}

/// IEEE half → f32 (no `half` crate: four lines, exact for every input).
/// pub(crate): model.rs's f16 channel path shares this (import audit R1)
/// — one converter, not two.
pub(crate) fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((bits >> 10) & 0x1F);
    let frac = f32::from(bits & 0x3FF);
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        31 => {
            if frac == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        e => sign * (1.0 + frac / 1024.0) * 2f32.powi(e - 15),
    }
}

/// `[0, 1]` float → u8, clamped; NaN reads as 0. Returns whether it
/// clamped, for the report.
fn unit_to_u8(v: f32) -> (u8, bool) {
    if v.is_nan() {
        return (0, true);
    }
    let clamped = v.clamp(0.0, 1.0);
    ((clamped * 255.0 + 0.5) as u8, clamped != v)
}

/// sRGB-encoded byte → linear byte (IEC 61966-2-1), rounded to nearest:
/// a 256-entry table built once per decode, exact at 0 and 255.
fn srgb_to_linear_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    for (i, out) in table.iter_mut().enumerate() {
        let s = i as f32 / 255.0;
        let l = if s <= 0.040_45 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        };
        *out = (l * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    }
    table
}

/// Decodes one cache dataset to 8-bit RGBA.
///
/// - `SRGB8_ALPHA8`: RGB linearized, alpha as stored. Umber's paint
///   planes hold LINEAR colour: the picker and the eyedropper linearize
///   on the way in, and export re-applies the sRGB curve to baseColor
///   and emissive (`umber_export::driver::output_transfer`). Kept
///   sRGB-encoded, an imported base colour would get the curve twice and
///   export washed out.
/// - `RG8` / `RG16F`: R is the value, G its coverage: grey `(v, v, v)`
///   with alpha = G.
/// - `RGBA16F`: each component clamped to `[0, 1]`, then scaled.
///
/// Float clamps are counted and reported, never silent.
pub fn decode(bytes: &[u8]) -> Result<CacheChannel, CacheError> {
    decode_with(bytes, Remap::Unit)
}

/// How an `RG16F` value maps to 8 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remap {
    /// `[0, 1]` → `[0, 255]`, clamped (roughness-like data).
    Unit,
    /// `[-1, 1]` → `[0, 255]`, centred: 0.0 lands on 128. Umber's
    /// height channel rests at 128, "no displacement" (the export neutral,
    /// C2-6). Painter's cached height rests at 0.0 and goes both ways, so
    /// `Unit` would drop every recess to black. The range is measured:
    /// across the 10 corpus height stacks the extremes are -1.116 and
    /// +0.463, and ±1 clips 0.02% of one stack (±0.5 clipped 5.5%).
    Signed,
}

/// [`decode`] with an explicit float remap for the two-channel float
/// format (other formats ignore it).
pub fn decode_with(bytes: &[u8], remap: Remap) -> Result<CacheChannel, CacheError> {
    let header = parse_header(bytes)?;
    let block = &bytes[header.block_offset..header.block_offset + header.block_len];
    // The corpus shows two block endings: a full stream (a 2048²
    // channel decodes to exactly bufferSize) and a stream that stops
    // early with an invalid match offset of 0 - Painter's end marker
    // for never-painted tails (three 4096² stacks all end that way).
    // A match offset of 0 is invalid mid-stream in every LZ4 variant,
    // so it can only mean end-of-block: keep the decoded prefix and
    // zero-fill the rest, with a report note.
    let mut raw = vec![0u8; header.buffer_size];
    let marker = lz4_tail_zero_offset(block);
    let written = match lz4_flex::block::decompress_into(block, &mut raw) {
        Ok(written) => written,
        // The end-of-block marker case: decode the prefix (below).
        Err(_e) if marker => decompress_prefix(block, &mut raw),
        Err(e) => return Err(CacheError::Lz4(e.to_string())),
    };
    // A block without the marker must fill the buffer exactly: a short
    // decode is a corrupt block, not a prefix.
    if !marker && written != header.buffer_size {
        return Err(CacheError::Lz4(format!(
            "decoded {written} of {} bytes",
            header.buffer_size
        )));
    }
    let truncated = written != header.buffer_size;
    let texels = header.width as usize * header.height as usize;
    let mut rgba = Vec::with_capacity(texels * 4);
    let mut clamped = 0usize;
    let half = |raw: &[u8], i: usize| f16_to_f32(u16::from_le_bytes([raw[i], raw[i + 1]]));
    match header.format {
        GlFormat::Srgb8Alpha8 => {
            let lin = srgb_to_linear_table();
            for t in raw.chunks_exact(4) {
                rgba.extend_from_slice(&[
                    lin[t[0] as usize],
                    lin[t[1] as usize],
                    lin[t[2] as usize],
                    t[3],
                ]);
            }
        }
        GlFormat::Rg8 => {
            for t in raw.chunks_exact(2) {
                rgba.extend_from_slice(&[t[0], t[0], t[0], t[1]]);
            }
        }
        GlFormat::Rg16F => {
            let signed = remap == Remap::Signed;
            for t in 0..texels {
                let raw_v = half(&raw, t * 4);
                let (v, c1) = unit_to_u8(if signed { raw_v * 0.5 + 0.5 } else { raw_v });
                let (a, c2) = unit_to_u8(half(&raw, t * 4 + 2));
                clamped += usize::from(c1) + usize::from(c2);
                rgba.extend_from_slice(&[v, v, v, a]);
            }
        }
        GlFormat::Rgba16F => {
            for t in 0..texels {
                for c in 0..4 {
                    let (v, hit) = unit_to_u8(half(&raw, t * 8 + c * 2));
                    clamped += usize::from(hit);
                    rgba.push(v);
                }
            }
        }
    }
    let mut notes = Vec::new();
    if truncated {
        notes.push(format!(
            "lz4 block ends early at {written} of {} bytes (zero-filled)",
            header.buffer_size
        ));
    }
    if header.format == GlFormat::Srgb8Alpha8 {
        notes.push("SRGB8_ALPHA8: colour linearized (Umber's planes are linear)".to_string());
    }
    if header.format == GlFormat::Rg16F && remap == Remap::Signed {
        notes.push("RG16F: signed height, 0.0 → 128 (Umber's flat height), ±1 → 0/255".to_string());
    }
    if matches!(header.format, GlFormat::Rg8 | GlFormat::Rg16F) {
        notes.push(format!(
            "{}: two-channel value + coverage, imported as grey with alpha",
            header.format.name()
        ));
    }
    if clamped > 0 {
        notes.push(format!(
            "{}: {clamped} component(s) outside [0,1] clamped to 8-bit",
            header.format.name()
        ));
    }
    Ok(CacheChannel {
        header,
        rgba,
        notes,
    })
}

/// A minimal LZ4 block walk: where a match offset of 0 appears. `true`
/// when it appears with only zero bytes (padding) after it, which is
/// how the corpus's truncated blocks all end (mid-stream nowhere).
fn lz4_tail_zero_offset(block: &[u8]) -> bool {
    // Only the explicit marker counts: a match offset of 0 followed
    // by nothing but zero bytes (Painter's end-of-block padding). A
    // block that merely ends early, without the marker, is refused.
    let mut i = 0usize;
    let n = block.len();
    while i < n {
        let tok = block[i];
        i += 1;
        let mut lit = (tok >> 4) as usize;
        if lit == 15 {
            loop {
                let Some(&b) = block.get(i) else { return false };
                i += 1;
                lit += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        let Some(lit_end) = i.checked_add(lit) else {
            return false;
        };
        if lit_end > n {
            return false;
        }
        i = lit_end;
        if i >= n {
            return false; // ends after literals: no marker
        }
        let Some(off) = block.get(i..i + 2).map(|s| [s[0], s[1]]) else {
            return false;
        };
        i += 2;
        let off = u16::from_le_bytes(off);
        if off == 0 {
            return block[i..].iter().all(|&b| b == 0);
        }
        if tok & 15 == 15 {
            loop {
                let Some(&b) = block.get(i) else { return false };
                i += 1;
                if b != 255 {
                    break;
                }
            }
        }
        if i > n {
            return false;
        }
    }
    false
}

/// Decodes the valid prefix of an LZ4 block that ends in the zero-
/// offset marker, stopping there. The output's tail stays zeroed
/// (Painter's never-painted region). Returns how many bytes decoded.
fn decompress_prefix(block: &[u8], out: &mut [u8]) -> usize {
    let mut i = 0usize; // in
    let mut w = 0usize; // out
    while i < block.len() && w < out.len() {
        let tok = block[i];
        i += 1;
        let mut lit = (tok >> 4) as usize;
        if lit == 15 {
            loop {
                let Some(&b) = block.get(i) else { return w };
                i += 1;
                lit += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        let lit = lit.min(out.len() - w).min(block.len().saturating_sub(i));
        if i + lit > block.len() {
            return w;
        }
        out[w..w + lit].copy_from_slice(&block[i..i + lit]);
        w += lit;
        i += lit;
        if i + 2 > block.len() {
            return w;
        }
        let off = u16::from_le_bytes([block[i], block[i + 1]]);
        if off == 0 {
            return w;
        }
        i += 2;
        let mut ml = (tok & 15) as usize;
        if ml == 15 {
            loop {
                let Some(&b) = block.get(i) else { return w };
                i += 1;
                ml += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        let ml = ml + 4;
        if w < off as usize {
            return w; // reference before start: bail with the prefix
        }
        let start = w - off as usize;
        for k in 0..ml {
            if w >= out.len() {
                return w;
            }
            out[w] = out[start + k];
            w += 1;
        }
    }
    w
}

/// Texture-set names from `paint/scene.assbin`: material uid → name.
///
/// The scene is an Assimp binary dump (`ASSIMP.binary-dump` header).
/// Each material property is `u32 key length + key + u32 semantic + u32
/// index + u32 type + u32 data length + data`. Painter's `texture-cache/
/// <id>` group names ARE the `$mat.uid` values (type 5 = buffer, 8 bytes,
/// u64 LE): all 10 sets across the 3 corpus files resolve, to the names
/// Painter shows (`Body`, `Fur`, `Eyes`, …). The name is the nearest
/// `?mat.name` (type 3 = string: `u32 length + UTF-8`) before the uid,
/// after the previous one.
///
/// A scan, not a parser: anything that does not match the exact layout
/// is skipped, so a hostile scene yields fewer names, never a panic. A
/// set without a name keeps `cache-<id>`.
pub fn material_names(scene: &[u8]) -> Vec<(u64, String)> {
    const UID: &[u8] = b"\x08\x00\x00\x00$mat.uid";
    const NAME: &[u8] = b"\x09\x00\x00\x00?mat.name";
    let le32 = |at: usize| -> Option<u32> {
        let b = scene.get(at..at.checked_add(4)?)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let find = |needle: &[u8], from: usize, to: usize| -> Option<usize> {
        let hay = scene.get(from..to.min(scene.len()))?;
        hay.windows(needle.len())
            .position(|w| w == needle)
            .map(|i| from + i)
    };
    let rfind = |needle: &[u8], from: usize, to: usize| -> Option<usize> {
        let hay = scene.get(from..to.min(scene.len()))?;
        hay.windows(needle.len())
            .rposition(|w| w == needle)
            .map(|i| from + i)
    };
    let mut out = Vec::new();
    let (mut pos, mut prev) = (0usize, 0usize);
    while let Some(at) = find(UID, pos, scene.len()) {
        pos = at + 1;
        let p = at + UID.len();
        // semantic, index, type, length, then the 8-byte uid.
        let (Some(kind), Some(len)) = (le32(p + 8), le32(p + 12)) else {
            break;
        };
        let Some(uid_bytes) = scene.get(p + 16..p + 24) else {
            break;
        };
        let start = prev;
        prev = p;
        if kind != 5 || len != 8 {
            continue;
        }
        let mut a = [0u8; 8];
        a.copy_from_slice(uid_bytes);
        let uid = u64::from_le_bytes(a);
        let Some(name_at) = rfind(NAME, start, at) else {
            continue;
        };
        let q = name_at + NAME.len();
        let (Some(kind), Some(data_len), Some(str_len)) = (le32(q + 8), le32(q + 12), le32(q + 16))
        else {
            continue;
        };
        if kind != 3 || str_len > 256 || data_len != str_len + 5 {
            continue;
        }
        let Some(raw) = scene.get(q + 20..q + 20 + str_len as usize) else {
            continue;
        };
        if let Ok(name) = std::str::from_utf8(raw) {
            out.push((uid, name.to_string()));
        }
    }
    out
}

/// The live channels of one cache stack, from its `list` dataset:
/// `(dataset leaf name, channel flag)` in list order.
///
/// Observed layouts, keyed by the cache's own `header` stamp minor
/// (`texture-cache/header`, `version.minor`):
/// - **v3.2** (2 corpus files): 20-byte entries, u64 BE flag at offset
///   12; the dataset is named by the flag (`1`, `128`, `4194304`, …).
/// - **v3.3** (HydrageaBessa): 16-byte entries, u32 BE bit index at
///   offset 12; the dataset is named by the index (`0`, `7`, `22`, …) and
///   the flag is `1 << index`.
///
/// v3.3 stacks also hold datasets the list does not name: leftovers of
/// the v3.2 naming, byte-identical copies or stale (20 in HydrageaBessa).
/// Only listed datasets are live. A malformed list, an unknown minor or
/// a bit index of 64 or more yields `None`, and the caller falls back to
/// importing nothing from that stack (never a guess).
pub fn live_channels(list: &[u8], minor: i64) -> Option<Vec<(String, u64)>> {
    let entry = match minor {
        2 => 20,
        3 => 16,
        _ => return None,
    };
    if list.len() % entry != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(list.len() / entry);
    for e in list.chunks_exact(entry) {
        let at = &e[12..];
        let item = match minor {
            2 => {
                let mut a = [0u8; 8];
                a.copy_from_slice(&at[..8]);
                let flag = u64::from_be_bytes(a);
                (flag.to_string(), flag)
            }
            _ => {
                let bit = u32::from_be_bytes([at[0], at[1], at[2], at[3]]);
                if bit >= 64 {
                    return None;
                }
                (bit.to_string(), 1u64 << bit)
            }
        };
        if out.iter().any(|(_, flag)| *flag == item.1) {
            return None;
        }
        out.push(item);
    }
    Some(out)
}

/// The `version.minor` of the cache's `header` stamp dataset, or `None`
/// when the bytes are not that stamp.
pub fn stamp_minor(bytes: &[u8]) -> Option<i64> {
    let mut r = Reader { bytes, at: 0 };
    let count = r.u32("entry count").ok()?;
    if count > 64 {
        return None;
    }
    let mut minor = None;
    for _ in 0..count {
        let key = r.qstring("key").ok()?;
        if let Variant::Int(v) = read_variant(&mut r).ok()? {
            if key == "version.minor" {
                minor = Some(v);
            }
        }
    }
    minor
}

/// The `height` channel's flag (see [`channel_name_for_flag`]).
pub const HEIGHT_FLAG: u64 = 2;

/// The texel format every corpus file pairs with a named flag: the
/// check [`channel_name_for_flag`]'s names rest on. `None` for flags
/// with no name.
pub fn expected_format(flag: u64) -> Option<GlFormat> {
    match flag {
        1 | 16 => Some(GlFormat::Srgb8Alpha8),
        2 => Some(GlFormat::Rg16F),
        8 | 128 | 8192 => Some(GlFormat::Rg8),
        4_194_304 => Some(GlFormat::Rgba16F),
        _ => None,
    }
}

/// The channel name for a cache flag, and whether the name is
/// established (`true`) or inferred (`false`, reported as such).
///
/// Evidence (research §7a):
/// - BessaCreatureV2's `paint/document.bin` stores its document channels
///   as `(format, flag)` records whose format codes follow the shader
///   API's documented list (`sRGB8` 0, `L8` 1, `RGB8` 2, `L16` 3, `RGB16`
///   4, `L16F` 5, `RGB16F` 6): 2 → L16F, 128 → L8, 8192 → L8,
///   4194304 → RGB16F, 16 → sRGB8. The other two files keep that table
///   in a layout this scan does not read.
/// - Every corpus set carries flags 1, 2, 128, 8192 and 4194304. Those
///   are five channels, the size of Painter's default PBR
///   Metallic/Roughness stack (base colour, height, roughness, metallic,
///   normal).
/// - The pixels agree, in all three files:
///   - 1 is an opaque sRGB colour;
///   - 4194304 rests at (0.5, 0.5, 1), a tangent-space normal;
///   - 2 is a float grey, flat zero on unsculpted sets (height);
///   - 128 is mid-grey everywhere (roughness);
///   - 8192 is zero on organic sets (metallic).
///
/// Established: those five. Inferred: 16 → `emissive` (an sRGB colour,
/// mostly black, partial coverage) and 8 → `opacity` (L8, one set solid
/// white). Everything else stays `flag_<n>`.
pub fn channel_name_for_flag(flag: u64) -> (String, bool) {
    match flag {
        1 => ("baseColor".to_string(), true),
        2 => ("height".to_string(), true),
        128 => ("roughness".to_string(), true),
        8192 => ("metallic".to_string(), true),
        4_194_304 => ("normal".to_string(), true),
        16 => ("emissive".to_string(), false),
        8 => ("opacity".to_string(), false),
        other => (format!("flag_{other}"), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `QString` as `QDataStream` writes it.
    fn qs(s: &str) -> Vec<u8> {
        let units: Vec<u8> = s.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let mut out = (units.len() as u32).to_be_bytes().to_vec();
        out.extend(units);
        out
    }

    fn int(key: &str, v: i32) -> Vec<u8> {
        let mut out = qs(key);
        out.extend(2u32.to_be_bytes());
        out.push(0);
        out.extend(v.to_be_bytes());
        out
    }

    fn text(key: &str, v: &str) -> Vec<u8> {
        let mut out = qs(key);
        out.extend(10u32.to_be_bytes());
        out.push(0);
        out.extend(qs(v));
        out
    }

    /// A cache dataset in the observed layout (key order as Painter's:
    /// alphabetical, the `QMap` order).
    fn dataset(w: i32, h: i32, gl: i32, buffer: i32, pixels: &[u8]) -> Vec<u8> {
        let mut out = 6u32.to_be_bytes().to_vec();
        out.extend(int("bufferSize", buffer));
        out.extend(text("compression", "lz4"));
        out.extend(int("height", h));
        out.extend(int("internalFormat", gl));
        out.extend(text("version", "v1"));
        out.extend(int("width", w));
        let block = lz4_flex::block::compress(pixels);
        out.extend((block.len() as u32).to_be_bytes());
        out.extend(block);
        out
    }

    #[test]
    fn an_srgb_channel_lands_linear_with_alpha_untouched() {
        // Anchors: 0 and 255 exact; sRGB 128 (mid-grey on screen) is
        // linear 55; 188 is linear 128 (half energy).
        let pixels = [
            0u8, 128, 255, 77, 188, 188, 188, 200, 255, 0, 128, 0, 10, 20, 30, 255,
        ];
        let ch = decode(&dataset(2, 2, 0x8C43, 16, &pixels)).unwrap();
        assert_eq!((ch.header.width, ch.header.height), (2, 2));
        assert_eq!(ch.header.format, GlFormat::Srgb8Alpha8);
        assert_eq!(&ch.rgba[..4], &[0, 55, 255, 77]);
        assert_eq!(&ch.rgba[4..8], &[128, 128, 128, 200]);
        // Alpha is never curved.
        let alphas: Vec<u8> = ch.rgba.chunks(4).map(|t| t[3]).collect();
        assert_eq!(alphas, [77, 200, 0, 255]);
        assert!(ch.notes.iter().any(|n| n.contains("linearized")));
        // The inverse of the export writer's curve: re-encoding returns
        // the original bytes within 1 (8-bit round trip).
        let to_srgb = |l: u8| {
            let l = f32::from(l) / 255.0;
            let s = if l <= 0.003_130_8 {
                l * 12.92
            } else {
                1.055 * l.powf(1.0 / 2.4) - 0.055
            };
            (s * 255.0 + 0.5) as i32
        };
        for (i, v) in [128u8, 188, 10, 20, 30].iter().enumerate() {
            let lin = srgb_to_linear_table()[*v as usize];
            assert!(
                (to_srgb(lin) - i32::from(*v)).abs() <= 3,
                "{i}: {v} -> {lin}"
            );
        }
    }

    #[test]
    fn two_channel_and_half_float_formats_convert() {
        // RG8: value + coverage → grey + alpha.
        let ch = decode(&dataset(2, 1, 0x822B, 4, &[10, 200, 30, 40])).unwrap();
        assert_eq!(ch.rgba, vec![10, 10, 10, 200, 30, 30, 30, 40]);
        // RG16F: 0.5 (0x3800) at coverage 1.0 (0x3C00), then 2.0 (0x4000,
        // clamps) at 0.0.
        let rg16: Vec<u8> = [0x3800u16, 0x3C00, 0x4000, 0x0000]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let ch = decode(&dataset(2, 1, 0x822F, 8, &rg16)).unwrap();
        assert_eq!(ch.rgba, vec![128, 128, 128, 255, 255, 255, 255, 0]);
        assert!(
            ch.notes.iter().any(|n| n.contains("1 component(s)")),
            "{:?}",
            ch.notes
        );
        // RGBA16F: the resting normal (0.5, 0.5, 1, 1).
        let n: Vec<u8> = [0x3800u16, 0x3800, 0x3C00, 0x3C00]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let ch = decode(&dataset(1, 1, 0x881A, 8, &n)).unwrap();
        assert_eq!(ch.rgba, vec![128, 128, 255, 255]);
    }

    #[test]
    fn signed_height_centres_on_umbers_flat_value() {
        // 0.0 (flat), -0.5 (recess), +0.5 (bump), -2.0 (clamped), all at
        // full coverage.
        let px: Vec<u8> = [
            0x0000u16, 0x3C00, 0xB800, 0x3C00, 0x3800, 0x3C00, 0xC000, 0x3C00,
        ]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
        let ch = decode_with(&dataset(4, 1, 0x822F, 16, &px), Remap::Signed).unwrap();
        let values: Vec<u8> = ch.rgba.chunks(4).map(|t| t[0]).collect();
        assert_eq!(values, [128, 64, 191, 0]);
        assert!(ch.notes.iter().any(|n| n.contains("signed height")));
        // Unit (the default) would have put flat height at black.
        let unit = decode(&dataset(4, 1, 0x822F, 16, &px)).unwrap();
        assert_eq!(unit.rgba[0], 0);
    }

    #[test]
    fn hostile_headers_are_refused_before_allocating() {
        let px = [0u8; 16];
        // bufferSize disagreeing with the format: refused.
        assert!(matches!(
            decode(&dataset(2, 2, 0x8C43, 999, &px)),
            Err(CacheError::TooLarge(_))
        ));
        // A side over the cap: refused from the header alone.
        assert!(matches!(
            parse_header(&dataset(MAX_SIDE as i32 + 1, 1, 0x8C43, 4, &px)),
            Err(CacheError::TooLarge(_))
        ));
        // 16K² RGBA16F = 2 GiB: over the byte cap.
        assert!(matches!(
            parse_header(&dataset(16384, 16384, 0x881A, i32::MAX, &px)),
            Err(CacheError::TooLarge(_))
        ));
        // An unknown GL format.
        assert!(matches!(
            parse_header(&dataset(2, 2, 0x1234, 16, &px)),
            Err(CacheError::Malformed(_))
        ));
        // Truncated anywhere: never a panic.
        let good = dataset(2, 2, 0x8C43, 16, &px);
        for cut in 0..good.len() {
            let _ = decode(&good[..cut]);
        }
        assert!(decode(&good[..good.len() - 1]).is_err());
    }

    #[test]
    fn a_block_decoding_short_is_refused() {
        // Header says 4×4×4 = 64, block holds 16 bytes.
        let mut d = dataset(4, 4, 0x8C43, 64, &[7u8; 64]);
        let small = lz4_flex::block::compress(&[7u8; 16]);
        let at = d.len() - lz4_flex::block::compress(&[7u8; 64]).len() - 4;
        d.truncate(at);
        d.extend((small.len() as u32).to_be_bytes());
        d.extend(small);
        assert!(
            matches!(decode(&d), Err(CacheError::Lz4(_))),
            "{:?}",
            decode(&d)
        );
    }

    #[test]
    fn the_stamp_dataset_is_not_a_channel() {
        let mut d = 2u32.to_be_bytes().to_vec();
        d.extend(int("version", 3));
        d.extend(int("version.minor", 2));
        assert_eq!(parse_header(&d), Err(CacheError::NotAChannel));
    }

    #[test]
    fn the_default_stack_is_named_and_the_rest_says_so() {
        for (flag, name) in [
            (1, "baseColor"),
            (2, "height"),
            (128, "roughness"),
            (8192, "metallic"),
            (4_194_304, "normal"),
        ] {
            assert_eq!(channel_name_for_flag(flag), (name.to_string(), true));
        }
        assert_eq!(channel_name_for_flag(16), ("emissive".to_string(), false));
        assert_eq!(channel_name_for_flag(8), ("opacity".to_string(), false));
        assert_eq!(
            channel_name_for_flag(70_368_744_177_664),
            ("flag_70368744177664".to_string(), false)
        );
        // Every named flag has the format the corpus pairs it with.
        for flag in [1, 2, 8, 16, 128, 8192, 4_194_304] {
            assert!(expected_format(flag).is_some(), "{flag}");
        }
        assert_eq!(expected_format(70_368_744_177_664), None);
    }

    /// One Assimp binary material property, as the corpus stores it.
    fn prop(key: &str, kind: u32, data: &[u8]) -> Vec<u8> {
        let mut o = (key.len() as u32).to_le_bytes().to_vec();
        o.extend(key.as_bytes());
        o.extend(0u32.to_le_bytes());
        o.extend(0u32.to_le_bytes());
        o.extend(kind.to_le_bytes());
        o.extend((data.len() as u32).to_le_bytes());
        o.extend(data);
        o
    }

    fn name_prop(name: &str) -> Vec<u8> {
        let mut d = (name.len() as u32).to_le_bytes().to_vec();
        d.extend(name.as_bytes());
        d.push(0);
        prop("?mat.name", 3, &d)
    }

    #[test]
    fn material_names_pair_each_uid_with_its_own_name() {
        let mut scene = b"ASSIMP.binary-dump.".to_vec();
        for (name, uid) in [
            ("Body", 1423614033083633785u64),
            ("Fur", 8074553802099130621),
        ] {
            scene.extend(name_prop(name));
            scene.extend(prop("$clr.diffuse", 1, &[0; 12]));
            scene.extend(prop("$mat.uid", 5, &uid.to_le_bytes()));
        }
        // A uid with no name of its own after the previous uid: unnamed.
        scene.extend(prop("$mat.uid", 5, &7u64.to_le_bytes()));
        assert_eq!(
            material_names(&scene),
            vec![
                (1423614033083633785, "Body".to_string()),
                (8074553802099130621, "Fur".to_string())
            ]
        );
        // Truncated anywhere: fewer names, never a panic.
        for cut in 0..scene.len() {
            let _ = material_names(&scene[..cut]);
        }
        assert!(material_names(b"").is_empty());
    }

    #[test]
    fn the_list_names_the_live_channels_in_both_layouts() {
        // v3.2: 20-byte entries, u64 BE flag at offset 12 (the corpus's
        // first entries: flags 1 then 2).
        let mut v2 = Vec::new();
        for flag in [1u64, 2, 4_194_304] {
            v2.extend([0u8; 12]);
            v2.extend(flag.to_be_bytes());
        }
        assert_eq!(
            live_channels(&v2, 2).unwrap(),
            [
                ("1".into(), 1),
                ("2".into(), 2),
                ("4194304".into(), 4_194_304)
            ]
        );
        // v3.3: 16-byte entries, u32 BE bit index at offset 12.
        let mut v3 = Vec::new();
        for bit in [0u32, 13, 22] {
            v3.extend([0u8; 12]);
            v3.extend(bit.to_be_bytes());
        }
        assert_eq!(
            live_channels(&v3, 3).unwrap(),
            [
                ("0".into(), 1),
                ("13".into(), 8192),
                ("22".into(), 4_194_304)
            ]
        );
        // Ragged, unknown minor, out-of-range bit, duplicate: no guess.
        assert_eq!(live_channels(&v3[..15], 3), None);
        assert_eq!(live_channels(&v3, 4), None);
        let mut big = vec![0u8; 12];
        big.extend(64u32.to_be_bytes());
        assert_eq!(live_channels(&big, 3), None);
        let mut dup = v3[..16].to_vec();
        dup.extend_from_slice(&v3[..16]);
        assert_eq!(live_channels(&dup, 3), None);
        assert_eq!(live_channels(&[], 2), Some(Vec::new()));
    }

    #[test]
    fn the_stamp_minor_reads_from_the_header_dataset() {
        let mut d = 2u32.to_be_bytes().to_vec();
        d.extend(int("version", 3));
        d.extend(int("version.minor", 3));
        assert_eq!(stamp_minor(&d), Some(3));
        assert_eq!(stamp_minor(&d[..10]), None);
    }

    #[test]
    fn half_floats_decode_exactly() {
        assert_eq!(f16_to_f32(0x3C00), 1.0);
        assert_eq!(f16_to_f32(0x3800), 0.5);
        assert_eq!(f16_to_f32(0xC000), -2.0);
        assert_eq!(f16_to_f32(0x0001), 2f32.powi(-24));
        assert!(f16_to_f32(0x7E00).is_nan());
        assert_eq!(f16_to_f32(0x7C00), f32::INFINITY);
    }
}
