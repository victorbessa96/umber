//! A clean-room SVG-subset parser and scanline rasterizer
//! (`docs/loop/briefs/2026-11-07-wave-svg-import-design.md`, row 266):
//! SVG bytes in, an 8-bit coverage plane out. Std + thiserror only, no
//! SVG/XML crate (same clean-room posture as the `.spp` reader).
//!
//! **The decision: rasterize at import.** No vector state persists. An
//! `.svg` stencil becomes a coverage plane once, at import, and from
//! there on it is byte-identical in behavior to a PNG stencil of the same
//! pixels: [`SvgRaster::to_rgba`] feeds
//! [`crate::stencil::StencilImage::from_alpha`] exactly like a decoded PNG
//! (white RGB, alpha = coverage). Editable paths are the path-tool wave's
//! problem, not the stencil's.
//!
//! What is parsed (everything else is out of scope for v1):
//!
//! - **XML subset**: a hand-rolled tokenizer. Start/end/self-closing tags,
//!   single- or double-quoted attributes, the five predefined entities
//!   (`&amp; &lt; &gt; &quot; &apos;`) plus numeric `&#NN;` / `&#xHH;`.
//!   Comments, `<?…?>` processing instructions, `<!DOCTYPE …>` (internal
//!   subset skipped, never expanded) and CDATA sections are skipped; text
//!   content is ignored. Well-formedness IS checked: a mismatched or
//!   unclosed tag, a duplicate attribute, an unknown entity, a raw `<` in
//!   an attribute value or content after the root element is
//!   [`SvgError::Parse`] with the byte offset into the input.
//! - **Elements**: `svg` (root only), `g`, `path`, `rect`, `circle`,
//!   `ellipse`, `polygon`, `polyline`, `line`. Element names match
//!   exactly (a namespace-prefixed `svg:rect` is an unknown element).
//! - **Presentation attributes**: `fill` (only `none` vs painted matters —
//!   the plane is coverage, so the color value, `currentColor` and even a
//!   `url(#paint)` reference all mean "painted"; `inherit` or an empty
//!   value keeps the parent's), `fill-rule` (`nonzero` default, `evenodd`;
//!   an unrecognized value keeps the parent's, the spec's
//!   invalid-presentation-value rule), `transform`. `g` (and the root
//!   `svg`) pass all three down; a child overrides.
//! - **Path data**: `M L m l H h V v C c S s Q q T t A a Z z`, absolute and
//!   relative, implicit command repetition (a repeated moveto is a
//!   lineto), scientific notation, comma/space separators, compact arc
//!   flags (`a1 1 0 01 5 5`). Path data is strict: any syntax error is
//!   [`SvgError::Parse`] (the spec's "render up to the error" is not
//!   implemented — a stencil half-drawn from a damaged file is worse than
//!   a refusal the artist sees).
//! - **Transforms**: `matrix`, `translate`, `scale`, `rotate(a [cx cy])`,
//!   `skewX`, `skewY`, composed left-to-right (`"A B"` maps a point through
//!   B first, then A) and inherited through `g`. A root `svg` transform is
//!   honored like a `g` transform, inside the viewBox mapping.
//!
//! What is NOT parsed: CSS in any form (`<style>` elements AND the `style`
//! attribute — a `style="fill:none"` is ignored, so such a shape renders
//! painted), `use`/`defs`/`symbol`, `text`, `image`, gradients/patterns
//! (as elements; a `fill="url(…)"` reference counts as painted, above),
//! strokes (coverage is fill only; a `line` therefore never covers
//! anything, though its coordinates are still validated), `opacity` /
//! `fill-opacity`, `display`/`visibility`, nested `svg`, `a`, `switch`.
//!
//! **Unknown-element policy: skip the subtree.** Any element outside the
//! list above (`metadata`, `title`, `defs`, `style`, `text`, editor
//! namespaces, …) is skipped together with everything inside it — real
//! files carry metadata, and refusing them would refuse most of what
//! Inkscape/Illustrator write. Children of a shape element are skipped
//! too. [`SvgError::Unsupported`] is reserved for constructs whose
//! silent skip would produce a WRONG size: a root (or shape) length in a
//! unit other than unitless/`px` (`mm`, `%`, `em`, …).
//!
//! Raster size and mapping:
//!
//! - **Size**: root `width`/`height` (rounded to the nearest px); one of
//!   them plus a `viewBox` derives the other from the viewBox aspect; no
//!   `width`/`height` → the viewBox w/h; neither → [`SvgError::NoSize`].
//!   A zero dimension → [`SvgError::ZeroSize`].
//! - **The cap runs before any allocation**: `width * height >
//!   max_pixels` is [`SvgError::TooLarge`], checked the moment the root
//!   tag is read, before a single child element is parsed or the raster
//!   is allocated.
//! - **[`DEFAULT_SVG_RASTER_PIXELS`] = 2048²**, not the PNG stencil's
//!   4096²: the design sized the SVG cap for a 4x4 f32 supersample buffer,
//!   16x the output plane (1 GiB at 4096²). This implementation streams
//!   instead — it keeps one row of subsample bits plus one row of hit
//!   counts (`5 * width` bytes) next to the output plane, so the real
//!   overshoot is far below that budget. The 2048² default stays as the
//!   design's conservative cap; callers that need more (the loader wave's
//!   `--size`) pass their own `max_pixels` to [`rasterize_svg`].
//! - **viewBox stretch mapping**: `viewBox="minx miny w h"` maps linearly
//!   onto the full raster — `(minx, miny)` lands at raster `(0, 0)` and
//!   the viewBox w/h scales independently on each axis to the raster w/h.
//!   `preserveAspectRatio` (meet/slice/alignment) is NOT implemented; a
//!   viewBox whose aspect differs from width/height is stretched. No
//!   viewBox → user units are raster px.
//!
//! Rasterization:
//!
//! - Geometry is transformed to device (raster px) space FIRST, then
//!   flattened there, so the flatness tolerance is in raster px whatever
//!   the transform: [`FLATNESS_TOLERANCE_PX`] = 0.05 px (a fifth of the
//!   0.25 px subsample pitch). Béziers split into a uniform segment count
//!   from Wang's bound (`n = ceil(sqrt(d(d-1)/8 · max|Δ²P| / tol))`);
//!   arcs and ellipses use the chord-sagitta bound against the device
//!   radius (the transform's largest singular value times the larger
//!   radius). Every curve is capped at [`MAX_SEGMENTS_PER_CURVE`].
//! - Arcs: endpoint parameterization → center parameterization (the SVG
//!   implementation-notes conversion, radii scaled up when too small),
//!   then a segment fan. Zero radius → a straight line; identical
//!   endpoints → omitted.
//! - Each shape's fill closes every subpath implicitly and fills under
//!   its own fill rule; shapes union into the plane (a subsample is
//!   covered if ANY painted shape covers it).
//! - 4x4 supersampling: subsample `(i, j)` of pixel `(x, y)` sits at
//!   `(x + (i + 0.5)/4, y + (j + 0.5)/4)`; inside-ness is half-open
//!   (`[x0, x1)` spans, `[y0, y1)` edges), so a shape edge on an integer
//!   coordinate covers the pixels on its far side exactly and nothing
//!   else. Coverage byte = `round(255 * hits / 16)`, rounding half up.
//! - Degenerate input never panics: `fill="none"` skips the shape;
//!   zero/negative-size rects, circles and ellipses are not rendered;
//!   zero-area or non-finite geometry contributes nothing.
//! - Parse-error offsets are byte offsets into the input; inside an
//!   attribute value they are exact unless the value carries entity
//!   references before the error (then they are relative to the raw
//!   value start plus the decoded position — approximate).

use std::f64::consts::TAU;

use thiserror::Error;

/// The default raster cap for [`rasterize_svg_default`]: 2048² px. The
/// design brief sized the SVG cap for a 4x4 f32 supersample buffer, which
/// is 16x the output (1 GiB at 4096²), hence 2048² rather than the PNG
/// stencil's 4096² (see the module docs for this implementation's real,
/// smaller overshoot).
pub const DEFAULT_SVG_RASTER_PIXELS: usize = 2048 * 2048;

/// Max deviation of a flattened curve from the true curve, in raster px.
pub const FLATNESS_TOLERANCE_PX: f64 = 0.05;

/// Upper bound on the line segments one curve (Bézier, arc, ellipse)
/// flattens to.
pub const MAX_SEGMENTS_PER_CURVE: usize = 1024;

/// Subsamples per pixel along each axis.
const SUPERSAMPLE: usize = 4;

/// Subsamples per pixel (4x4).
const SUBSAMPLES: u32 = (SUPERSAMPLE * SUPERSAMPLE) as u32;

/// The most shapes one document may contribute (hostile-input budget).
///
/// The raster cap bounds the output, not the work: a million tiny paths
/// fit any raster and still cost a million shape builds, a million edge
/// sorts and a per-row scan over a million shapes.
pub const MAX_SHAPES: usize = 65_536;

/// The most flattened points one shape may contribute (hostile-input
/// budget). Curve flattening adds up to
/// [`MAX_SEGMENTS_PER_CURVE`] points per curve, so a path data string
/// with many curves can multiply fast; this bounds the geometry one
/// shape hands to the edge builder.
pub const MAX_POINTS_PER_SHAPE: usize = 262_144;

/// Everything that can go wrong rasterizing an SVG.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SvgError {
    /// Malformed XML, path data, transform, number or entity.
    #[error("SVG parse error at byte {offset}: {message}")]
    Parse {
        /// Byte offset into the input where the problem was found.
        offset: usize,
        /// What was wrong.
        message: String,
    },
    /// The root `svg` carries neither `width`/`height` nor a `viewBox`.
    #[error("SVG has no size: the root <svg> has neither width/height nor a viewBox")]
    NoSize,
    /// The declared raster size has a zero dimension.
    #[error("empty SVG raster ({width}x{height})")]
    ZeroSize {
        /// The raster width.
        width: u32,
        /// The raster height.
        height: u32,
    },
    /// The declared raster size exceeds the pixel cap.
    #[error("SVG raster {width}x{height} exceeds the {max_pixels}-pixel cap")]
    TooLarge {
        /// The raster width (saturated at `u32::MAX`).
        width: u32,
        /// The raster height (saturated at `u32::MAX`).
        height: u32,
        /// The cap that refused it.
        max_pixels: usize,
    },
    /// A construct whose skip would produce a wrong result (a non-px unit).
    #[error("unsupported SVG construct: {what}")]
    Unsupported {
        /// The construct, quoted from the input.
        what: String,
    },
    /// The document exceeds a structural budget (shapes or points).
    ///
    /// The raster-size cap bounds the OUTPUT, not the input's work: a
    /// file with a million tiny paths fits any raster and still costs a
    /// million shape builds, a million edge sorts and a per-row scan
    /// over a million shapes. These budgets refuse that class before
    /// the geometry is built.
    #[error("SVG is too complex: more than {limit} {what}")]
    TooComplex {
        /// What was counted ("shapes" or "path points").
        what: &'static str,
        /// The budget that refused it.
        limit: usize,
    },
}

/// A rasterized SVG: an 8-bit coverage plane, `width * height` bytes,
/// row-major, `255` = fully covered. The fields are private so
/// `coverage.len() == width * height` holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgRaster {
    width: u32,
    height: u32,
    coverage: Vec<u8>,
}

impl SvgRaster {
    /// Width in raster px.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in raster px.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The `width * height` coverage bytes, row-major.
    pub fn coverage(&self) -> &[u8] {
        &self.coverage
    }

    /// Tightly-packed RGBA8 (the PNG loader's shape): `r = g = b = 255`,
    /// `a = coverage`.
    pub fn to_rgba(&self) -> Vec<u8> {
        self.coverage
            .iter()
            .flat_map(|&a| [255, 255, 255, a])
            .collect()
    }
}

/// [`rasterize_svg`] under [`DEFAULT_SVG_RASTER_PIXELS`].
///
/// # Errors
///
/// As [`rasterize_svg`].
pub fn rasterize_svg_default(bytes: &[u8]) -> Result<SvgRaster, SvgError> {
    rasterize_svg(bytes, DEFAULT_SVG_RASTER_PIXELS)
}

/// Parses `bytes` as an SVG document (the subset in the module docs) and
/// rasterizes it to a coverage plane.
///
/// # Errors
///
/// [`SvgError::Parse`] for malformed input (including non-UTF-8),
/// [`SvgError::NoSize`] / [`SvgError::ZeroSize`] when there is no raster
/// size, [`SvgError::TooLarge`] when `width * height > max_pixels`
/// (checked before anything is allocated), [`SvgError::Unsupported`] for
/// non-px lengths.
pub fn rasterize_svg(bytes: &[u8], max_pixels: usize) -> Result<SvgRaster, SvgError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| parse_err(e.valid_up_to(), "the input is not valid UTF-8"))?;
    let (base, text) = match text.strip_prefix('\u{feff}') {
        Some(rest) => ('\u{feff}'.len_utf8(), rest),
        None => (0, text),
    };
    // The cap check lives inside parse_document, at the root tag; the
    // plane is allocated only below, after it passed.
    let doc = parse_document(text, base, max_pixels)?;
    let coverage = rasterize(doc.width, doc.height, &doc.shapes);
    Ok(SvgRaster {
        width: doc.width,
        height: doc.height,
        coverage,
    })
}

fn parse_err(offset: usize, message: impl Into<String>) -> SvgError {
    SvgError::Parse {
        offset,
        message: message.into(),
    }
}

// ---------------------------------------------------------------------------
// XML tokenizer
// ---------------------------------------------------------------------------

/// One attribute, value entity-decoded.
#[derive(Debug, Clone)]
struct Attr {
    name: String,
    value: String,
    /// Byte offset of the raw value's first byte in the input.
    value_offset: usize,
}

#[derive(Debug)]
enum Event {
    Start {
        name: String,
        attrs: Vec<Attr>,
        self_closing: bool,
        offset: usize,
    },
    End,
    Eof,
}

struct Tokenizer<'a> {
    src: &'a str,
    /// Added to every reported offset (a stripped BOM).
    base: usize,
    pos: usize,
    /// Open elements: name + offset of the `<`.
    open: Vec<(String, usize)>,
    root_closed: bool,
}

impl<'a> Tokenizer<'a> {
    fn new(src: &'a str, base: usize) -> Self {
        Self {
            src,
            base,
            pos: 0,
            open: Vec::new(),
            root_closed: false,
        }
    }

    fn rest(&self) -> &'a str {
        self.src.get(self.pos..).unwrap_or("")
    }

    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn err(&self, pos: usize, message: impl Into<String>) -> SvgError {
        parse_err(self.base + pos, message)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b) if b.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    /// Skips from the current `opener` to just past `closer`.
    fn skip_past(&mut self, opener: &str, closer: &str, what: &str) -> Result<(), SvgError> {
        let start = self.pos;
        let body = self.src.get(start + opener.len()..).unwrap_or("");
        match body.find(closer) {
            Some(i) => {
                self.pos = start + opener.len() + i + closer.len();
                Ok(())
            }
            None => Err(self.err(start, format!("unterminated {what}"))),
        }
    }

    /// Skips a `<!DOCTYPE …>`, internal subset and quoted strings
    /// included.
    fn skip_doctype(&mut self) -> Result<(), SvgError> {
        let start = self.pos;
        let bytes = self.src.as_bytes();
        let mut depth = 0usize;
        let mut quote: Option<u8> = None;
        let mut i = start + 2;
        while let Some(&b) = bytes.get(i) {
            match quote {
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None => match b {
                    b'"' | b'\'' => quote = Some(b),
                    b'[' => depth += 1,
                    b']' => depth = depth.saturating_sub(1),
                    b'>' if depth == 0 => {
                        self.pos = i + 1;
                        return Ok(());
                    }
                    _ => {}
                },
            }
            i += 1;
        }
        Err(self.err(start, "unterminated <!…> declaration"))
    }

    /// An XML name: everything up to whitespace or a tag delimiter. The
    /// delimiters are ASCII, so the slice bounds are char boundaries.
    fn read_name(&mut self) -> &'a str {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() || matches!(b, b'/' | b'>' | b'=' | b'<' | b'"' | b'\'') {
                break;
            }
            self.pos += 1;
        }
        self.src.get(start..self.pos).unwrap_or("")
    }

    fn next_event(&mut self) -> Result<Event, SvgError> {
        loop {
            let Some(lt) = self.rest().find('<') else {
                if let Some((name, offset)) = self.open.last() {
                    return Err(parse_err(
                        self.base + offset,
                        format!("unclosed element <{name}>"),
                    ));
                }
                self.pos = self.src.len();
                return Ok(Event::Eof);
            };
            self.pos += lt;
            let rest = self.rest();
            if rest.starts_with("<!--") {
                self.skip_past("<!--", "-->", "comment")?;
            } else if rest.starts_with("<![CDATA[") {
                self.skip_past("<![CDATA[", "]]>", "CDATA section")?;
            } else if rest.starts_with("<?") {
                self.skip_past("<?", "?>", "processing instruction")?;
            } else if rest.starts_with("<!") {
                self.skip_doctype()?;
            } else if rest.starts_with("</") {
                return self.end_tag();
            } else {
                return self.start_tag();
            }
        }
    }

    fn end_tag(&mut self) -> Result<Event, SvgError> {
        let start = self.pos;
        self.pos += 2;
        let name = self.read_name();
        self.skip_ws();
        if self.peek() != Some(b'>') {
            return Err(self.err(self.pos, format!("expected '>' to close </{name}")));
        }
        self.pos += 1;
        match self.open.pop() {
            Some((open_name, _)) if open_name == name => {}
            Some((open_name, _)) => {
                return Err(self.err(start, format!("</{name}> closes <{open_name}>")));
            }
            None => return Err(self.err(start, format!("unexpected </{name}>"))),
        }
        if self.open.is_empty() {
            self.root_closed = true;
        }
        Ok(Event::End)
    }

    fn start_tag(&mut self) -> Result<Event, SvgError> {
        let start = self.pos;
        if self.root_closed {
            return Err(self.err(start, "content after the root element"));
        }
        self.pos += 1;
        let name = self.read_name();
        if name.is_empty() {
            return Err(self.err(self.pos, "expected an element name after '<'"));
        }
        let mut attrs: Vec<Attr> = Vec::new();
        let self_closing = loop {
            self.skip_ws();
            match self.peek() {
                None => return Err(self.err(start, format!("unterminated tag <{name}"))),
                Some(b'>') => {
                    self.pos += 1;
                    break false;
                }
                Some(b'/') => {
                    if self.rest().starts_with("/>") {
                        self.pos += 2;
                        break true;
                    }
                    return Err(self.err(self.pos, "expected '/>'"));
                }
                Some(_) => {
                    let attr = self.attribute()?;
                    if attrs.iter().any(|a| a.name == attr.name) {
                        let attr_name = attr.name;
                        return Err(self.err(
                            attr.value_offset - self.base,
                            format!("duplicate attribute {attr_name} on <{name}>"),
                        ));
                    }
                    attrs.push(attr);
                }
            }
        };
        if !self_closing {
            self.open.push((name.to_string(), start));
        } else if self.open.is_empty() {
            self.root_closed = true;
        }
        Ok(Event::Start {
            name: name.to_string(),
            attrs,
            self_closing,
            offset: self.base + start,
        })
    }

    fn attribute(&mut self) -> Result<Attr, SvgError> {
        let name_pos = self.pos;
        let name = self.read_name();
        if name.is_empty() {
            return Err(self.err(name_pos, "expected an attribute name"));
        }
        self.skip_ws();
        if self.peek() != Some(b'=') {
            return Err(self.err(self.pos, format!("expected '=' after attribute {name}")));
        }
        self.pos += 1;
        self.skip_ws();
        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => q,
            _ => return Err(self.err(self.pos, "expected a quoted attribute value")),
        };
        self.pos += 1;
        let value_start = self.pos;
        let Some(len) = self.rest().find(char::from(quote)) else {
            return Err(self.err(value_start - 1, "unterminated attribute value"));
        };
        let raw = self
            .src
            .get(value_start..value_start + len)
            .unwrap_or_default();
        self.pos = value_start + len + 1;
        let value = decode_entities(raw, self.base + value_start)?;
        Ok(Attr {
            name: name.to_string(),
            value,
            value_offset: self.base + value_start,
        })
    }
}

/// Decodes the predefined entities and numeric character references in
/// an attribute value; `offset` is the value's position in the input.
fn decode_entities(raw: &str, offset: usize) -> Result<String, SvgError> {
    let mut out = String::with_capacity(raw.len());
    let mut consumed = 0usize;
    let mut rest = raw;
    while let Some(i) = rest.find(['&', '<']) {
        out.push_str(rest.get(..i).unwrap_or_default());
        let at = offset + consumed + i;
        let tail = rest.get(i..).unwrap_or_default();
        if tail.starts_with('<') {
            return Err(parse_err(at, "'<' inside an attribute value"));
        }
        let Some(semi) = tail.find(';') else {
            return Err(parse_err(at, "unterminated entity reference"));
        };
        let entity = tail.get(1..semi).unwrap_or_default();
        let ch = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => {
                let code = if let Some(hex) = entity.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = entity.strip_prefix('#') {
                    dec.parse::<u32>().ok()
                } else {
                    None
                };
                code.and_then(char::from_u32)
            }
        };
        let Some(ch) = ch else {
            return Err(parse_err(at, format!("unknown entity &{entity};")));
        };
        out.push(ch);
        consumed += i + semi + 1;
        rest = tail.get(semi + 1..).unwrap_or_default();
    }
    out.push_str(rest);
    Ok(out)
}

fn find_attr<'a>(attrs: &'a [Attr], key: &str) -> Option<&'a Attr> {
    attrs.iter().find(|a| a.name == key)
}

// ---------------------------------------------------------------------------
// Numbers, lengths, cursors
// ---------------------------------------------------------------------------

/// Scans one SVG number at `pos`: an optional sign, digits with an
/// optional fraction (at least one digit overall), an optional exponent.
/// Returns the value and the end position; `None` when there is no
/// number there or it is not finite.
fn scan_number(s: &[u8], pos: usize) -> Option<(f64, usize)> {
    let digit = |i: usize| s.get(i).is_some_and(u8::is_ascii_digit);
    let mut i = pos;
    if matches!(s.get(i), Some(b'+' | b'-')) {
        i += 1;
    }
    let mut digits = 0usize;
    while digit(i) {
        i += 1;
        digits += 1;
    }
    if s.get(i) == Some(&b'.') {
        i += 1;
        while digit(i) {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(s.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if digit(j) {
            while digit(j) {
                j += 1;
            }
            i = j;
        }
    }
    let text = std::str::from_utf8(s.get(pos..i)?).ok()?;
    let value: f64 = text.parse().ok()?;
    value.is_finite().then_some((value, i))
}

/// A byte cursor over an attribute value (path data, points, transforms).
struct Cursor<'a> {
    s: &'a [u8],
    pos: usize,
    /// The value's offset in the input.
    base: usize,
}

impl<'a> Cursor<'a> {
    fn new(attr: &'a Attr) -> Self {
        Self {
            s: attr.value.as_bytes(),
            pos: 0,
            base: attr.value_offset,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b) if b.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    /// Whitespace, at most one comma, whitespace.
    fn skip_sep(&mut self) {
        self.skip_ws();
        if self.peek() == Some(b',') {
            self.pos += 1;
            self.skip_ws();
        }
    }

    fn at_number_start(&self) -> bool {
        matches!(self.peek(), Some(b'0'..=b'9' | b'.' | b'+' | b'-'))
    }

    fn error(&self, message: impl Into<String>) -> SvgError {
        parse_err(self.base + self.pos, message)
    }

    fn number(&mut self) -> Result<f64, SvgError> {
        self.skip_sep();
        match scan_number(self.s, self.pos) {
            Some((value, end)) => {
                self.pos = end;
                Ok(value)
            }
            None => Err(self.error("expected a number")),
        }
    }

    fn pair(&mut self) -> Result<[f64; 2], SvgError> {
        Ok([self.number()?, self.number()?])
    }

    fn flag(&mut self) -> Result<bool, SvgError> {
        self.skip_sep();
        let flag = match self.peek() {
            Some(b'0') => false,
            Some(b'1') => true,
            _ => return Err(self.error("expected an arc flag (0 or 1)")),
        };
        self.pos += 1;
        Ok(flag)
    }
}

/// A length: a number, unitless or `px`. Other units are
/// [`SvgError::Unsupported`] (a skip would silently mis-size).
fn parse_length(attr: &Attr, element: &str) -> Result<f64, SvgError> {
    let lead = attr.value.len() - attr.value.trim_start().len();
    let raw = attr.value.trim();
    let Some((value, end)) = scan_number(raw.as_bytes(), 0) else {
        let attr_name = &attr.name;
        return Err(parse_err(
            attr.value_offset + lead,
            format!("<{element}> {attr_name}: expected a number"),
        ));
    };
    let unit = raw.get(end..).unwrap_or_default().trim();
    if unit.is_empty() || unit == "px" {
        Ok(value)
    } else {
        let attr_name = &attr.name;
        let attr_value = &attr.value;
        Err(SvgError::Unsupported {
            what: format!("<{element}> {attr_name}=\"{attr_value}\" (only unitless or px lengths)"),
        })
    }
}

fn length_attr(attrs: &[Attr], key: &str, element: &str) -> Result<Option<f64>, SvgError> {
    find_attr(attrs, key)
        .map(|attr| parse_length(attr, element))
        .transpose()
}

// ---------------------------------------------------------------------------
// Transforms
// ---------------------------------------------------------------------------

/// A 2D affine map `[a b c d e f]`: `x' = a x + c y + e`,
/// `y' = b x + d y + f` (the SVG `matrix` order).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Affine([f64; 6]);

impl Affine {
    const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    fn translate(tx: f64, ty: f64) -> Self {
        Self([1.0, 0.0, 0.0, 1.0, tx, ty])
    }

    fn scale(sx: f64, sy: f64) -> Self {
        Self([sx, 0.0, 0.0, sy, 0.0, 0.0])
    }

    /// Rotation by `degrees` (positive = clockwise on the y-down raster).
    fn rotate(degrees: f64) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self([cos, sin, -sin, cos, 0.0, 0.0])
    }

    /// `self ∘ inner`: maps a point through `inner` first, then `self`.
    fn compose(self, inner: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = inner.0;
        Self([
            a * a2 + c * b2,
            b * a2 + d * b2,
            a * c2 + c * d2,
            b * c2 + d * d2,
            a * e2 + c * f2 + e,
            b * e2 + d * f2 + f,
        ])
    }

    fn apply(self, [x, y]: [f64; 2]) -> [f64; 2] {
        let [a, b, c, d, e, f] = self.0;
        [a * x + c * y + e, b * x + d * y + f]
    }

    /// The largest singular value of the linear part: how far a unit
    /// length can stretch (the device radius of a user-space circle).
    fn max_scale(self) -> f64 {
        let [a, b, c, d, _, _] = self.0;
        let t = a * a + b * b + c * c + d * d;
        let det = a * d - b * c;
        ((t + (t * t - 4.0 * det * det).max(0.0).sqrt()) / 2.0).sqrt()
    }
}

/// Parses a `transform` attribute (a list composed left-to-right).
fn parse_transform(attr: &Attr) -> Result<Affine, SvgError> {
    let mut cur = Cursor::new(attr);
    let mut m = Affine::IDENTITY;
    loop {
        cur.skip_sep();
        if cur.peek().is_none() {
            break;
        }
        let name_start = cur.pos;
        while matches!(cur.peek(), Some(b) if b.is_ascii_alphabetic()) {
            cur.pos += 1;
        }
        let name = cur
            .s
            .get(name_start..cur.pos)
            .and_then(|n| std::str::from_utf8(n).ok())
            .unwrap_or_default();
        if name.is_empty() {
            return Err(cur.error("expected a transform function"));
        }
        cur.skip_ws();
        if cur.peek() != Some(b'(') {
            return Err(cur.error("expected '(' after a transform name"));
        }
        cur.pos += 1;
        let mut args: Vec<f64> = Vec::new();
        loop {
            cur.skip_sep();
            if cur.peek() == Some(b')') {
                cur.pos += 1;
                break;
            }
            if args.len() == 6 {
                return Err(cur.error("too many transform arguments"));
            }
            args.push(cur.number()?);
        }
        let t = match (name, args.as_slice()) {
            ("matrix", &[a, b, c, d, e, f]) => Affine([a, b, c, d, e, f]),
            ("translate", &[tx]) => Affine::translate(tx, 0.0),
            ("translate", &[tx, ty]) => Affine::translate(tx, ty),
            ("scale", &[s]) => Affine::scale(s, s),
            ("scale", &[sx, sy]) => Affine::scale(sx, sy),
            ("rotate", &[angle]) => Affine::rotate(angle),
            ("rotate", &[angle, cx, cy]) => Affine::translate(cx, cy)
                .compose(Affine::rotate(angle))
                .compose(Affine::translate(-cx, -cy)),
            ("skewX", &[angle]) => Affine([1.0, 0.0, angle.to_radians().tan(), 1.0, 0.0, 0.0]),
            ("skewY", &[angle]) => Affine([1.0, angle.to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
            _ => {
                let count = args.len();
                return Err(parse_err(
                    cur.base + name_start,
                    format!("unsupported transform {name}() with {count} argument(s)"),
                ));
            }
        };
        m = m.compose(t);
    }
    Ok(m)
}

// ---------------------------------------------------------------------------
// Geometry: flattening into device-space polygons
// ---------------------------------------------------------------------------

/// Wang/sagitta estimate → a segment count in `1..=MAX_SEGMENTS_PER_CURVE`.
fn segment_count(raw: f64) -> usize {
    if raw.is_nan() {
        return 1;
    }
    raw.ceil().clamp(1.0, MAX_SEGMENTS_PER_CURVE as f64) as usize
}

/// Segments for an arc of device radius `radius` sweeping `sweep`
/// radians so the chord sagitta stays within [`FLATNESS_TOLERANCE_PX`].
fn arc_segment_count(radius: f64, sweep: f64) -> usize {
    let ratio = (1.0 - FLATNESS_TOLERANCE_PX / radius).clamp(-1.0, 1.0);
    let step = 2.0 * ratio.acos();
    if step.is_nan() || step <= 0.0 {
        return MAX_SEGMENTS_PER_CURVE;
    }
    segment_count(sweep.abs() / step)
}

fn second_difference(p0: [f64; 2], p1: [f64; 2], p2: [f64; 2]) -> f64 {
    (p0[0] - 2.0 * p1[0] + p2[0]).hypot(p0[1] - 2.0 * p1[1] + p2[1])
}

/// One elliptical-arc segment in endpoint parameterization (user space).
#[derive(Debug, Clone, Copy)]
struct ArcSeg {
    rx: f64,
    ry: f64,
    x_axis_rotation_deg: f64,
    large_arc: bool,
    sweep: bool,
    to: [f64; 2],
}

/// Accumulates one shape's subpaths as device-space polygons.
struct PolyBuilder {
    ctm: Affine,
    /// `ctm.max_scale()`, cached for the arc segment counts.
    scale: f64,
    polys: Vec<Vec<[f64; 2]>>,
    current: Vec<[f64; 2]>,
}

impl PolyBuilder {
    fn new(ctm: Affine) -> Self {
        Self {
            ctm,
            scale: ctm.max_scale(),
            polys: Vec::new(),
            current: Vec::new(),
        }
    }

    /// Ends the current subpath (fill closes it implicitly); fewer than
    /// three points enclose no area and are dropped.
    fn flush(&mut self) {
        if self.current.len() >= 3 {
            self.polys.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
    }

    /// Starts a subpath at `from` if none is open (a segment right after
    /// a closepath starts at the closed subpath's start).
    fn begin(&mut self, from: [f64; 2]) {
        if self.current.is_empty() {
            self.current.push(self.ctm.apply(from));
        }
    }

    fn move_to(&mut self, p: [f64; 2]) {
        self.flush();
        self.current.push(self.ctm.apply(p));
    }

    fn line_to(&mut self, from: [f64; 2], to: [f64; 2]) {
        self.begin(from);
        self.current.push(self.ctm.apply(to));
    }

    fn quad_to(&mut self, from: [f64; 2], ctrl: [f64; 2], to: [f64; 2]) {
        self.begin(from);
        let ctm = self.ctm;
        let [p0, p1, p2] = [from, ctrl, to].map(|p| ctm.apply(p));
        let dd = second_difference(p0, p1, p2);
        let n = segment_count((0.25 * dd / FLATNESS_TOLERANCE_PX).sqrt());
        for i in 1..n {
            let t = i as f64 / n as f64;
            let mt = 1.0 - t;
            let (w0, w1, w2) = (mt * mt, 2.0 * mt * t, t * t);
            self.current.push([
                w0 * p0[0] + w1 * p1[0] + w2 * p2[0],
                w0 * p0[1] + w1 * p1[1] + w2 * p2[1],
            ]);
        }
        self.current.push(p2);
    }

    fn cubic_to(&mut self, from: [f64; 2], c1: [f64; 2], c2: [f64; 2], to: [f64; 2]) {
        self.begin(from);
        let ctm = self.ctm;
        let [p0, p1, p2, p3] = [from, c1, c2, to].map(|p| ctm.apply(p));
        let dd = second_difference(p0, p1, p2).max(second_difference(p1, p2, p3));
        let n = segment_count((0.75 * dd / FLATNESS_TOLERANCE_PX).sqrt());
        for i in 1..n {
            let t = i as f64 / n as f64;
            let mt = 1.0 - t;
            let (w0, w1, w2, w3) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
            self.current.push([
                w0 * p0[0] + w1 * p1[0] + w2 * p2[0] + w3 * p3[0],
                w0 * p0[1] + w1 * p1[1] + w2 * p2[1] + w3 * p3[1],
            ]);
        }
        self.current.push(p3);
    }

    /// The SVG implementation-notes endpoint → center conversion, then a
    /// segment fan (points computed in user space, then mapped).
    fn arc_to(&mut self, from: [f64; 2], arc: &ArcSeg) {
        let [x1, y1] = from;
        let [x2, y2] = arc.to;
        if x1 == x2 && y1 == y2 {
            return;
        }
        let mut rx = arc.rx.abs();
        let mut ry = arc.ry.abs();
        if rx == 0.0 || ry == 0.0 {
            self.line_to(from, arc.to);
            return;
        }
        let (sin_phi, cos_phi) = arc.x_axis_rotation_deg.to_radians().sin_cos();
        let dx2 = (x1 - x2) / 2.0;
        let dy2 = (y1 - y2) / 2.0;
        let x1p = cos_phi * dx2 + sin_phi * dy2;
        let y1p = -sin_phi * dx2 + cos_phi * dy2;
        let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
        if lambda > 1.0 {
            let grow = lambda.sqrt();
            rx *= grow;
            ry *= grow;
        }
        let (rx2, ry2) = (rx * rx, ry * ry);
        let num = rx2 * ry2 - rx2 * y1p * y1p - ry2 * x1p * x1p;
        let den = rx2 * y1p * y1p + ry2 * x1p * x1p;
        let mut coef = if den > 0.0 {
            (num / den).max(0.0).sqrt()
        } else {
            0.0
        };
        if arc.large_arc == arc.sweep {
            coef = -coef;
        }
        let cxp = coef * rx * y1p / ry;
        let cyp = -coef * ry * x1p / rx;
        let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
        let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;
        let theta1 = ((y1p - cyp) / ry).atan2((x1p - cxp) / rx);
        let theta2 = ((-y1p - cyp) / ry).atan2((-x1p - cxp) / rx);
        let mut dtheta = theta2 - theta1;
        if arc.sweep && dtheta < 0.0 {
            dtheta += TAU;
        } else if !arc.sweep && dtheta > 0.0 {
            dtheta -= TAU;
        }
        let n = arc_segment_count(rx.max(ry) * self.scale, dtheta);
        self.begin(from);
        for i in 1..n {
            let theta = theta1 + dtheta * (i as f64 / n as f64);
            let (sin, cos) = theta.sin_cos();
            let p = [
                cx + rx * cos_phi * cos - ry * sin_phi * sin,
                cy + rx * sin_phi * cos + ry * cos_phi * sin,
            ];
            self.current.push(self.ctm.apply(p));
        }
        self.current.push(self.ctm.apply(arc.to));
    }

    /// A full axis-aligned ellipse as its own closed subpath.
    fn ellipse(&mut self, [cx, cy]: [f64; 2], rx: f64, ry: f64) {
        self.flush();
        let n = arc_segment_count(rx.max(ry) * self.scale, TAU).max(3);
        for i in 0..n {
            let (sin, cos) = (TAU * (i as f64 / n as f64)).sin_cos();
            self.current
                .push(self.ctm.apply([cx + rx * cos, cy + ry * sin]));
        }
        self.flush();
    }

    fn finish(mut self) -> Vec<Vec<[f64; 2]>> {
        self.flush();
        self.polys
    }
}

fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

/// `ctrl` mirrored through `about` (the S/T smooth control point).
fn reflect(ctrl: [f64; 2], about: [f64; 2]) -> [f64; 2] {
    [2.0 * about[0] - ctrl[0], 2.0 * about[1] - ctrl[1]]
}

/// Parses path data `d` into the builder.
fn build_path(d: &Attr, b: &mut PolyBuilder) -> Result<(), SvgError> {
    let mut cur = Cursor::new(d);
    let mut command: Option<u8> = None;
    let mut pos = [0.0, 0.0];
    let mut start = [0.0, 0.0];
    let mut last_cubic: Option<[f64; 2]> = None;
    let mut last_quad: Option<[f64; 2]> = None;
    loop {
        cur.skip_sep();
        let Some(byte) = cur.peek() else { break };
        let explicit = byte.is_ascii_alphabetic();
        if explicit {
            if command.is_none() && !matches!(byte, b'M' | b'm') {
                return Err(cur.error("path data must begin with a moveto"));
            }
            cur.pos += 1;
            command = Some(byte);
        }
        let Some(cmd) = command else {
            return Err(cur.error("path data must begin with a moveto"));
        };
        if !explicit && (!cur.at_number_start() || matches!(cmd, b'Z' | b'z')) {
            let found = char::from(byte);
            return Err(cur.error(format!("unexpected '{found}' in path data")));
        }
        let origin = if cmd.is_ascii_lowercase() {
            pos
        } else {
            [0.0, 0.0]
        };
        let mut next_cubic = None;
        let mut next_quad = None;
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let p = add(origin, cur.pair()?);
                b.move_to(p);
                pos = p;
                start = p;
                // A repeated moveto pair is an implicit lineto.
                command = Some(if cmd == b'm' { b'l' } else { b'L' });
            }
            b'L' => {
                let p = add(origin, cur.pair()?);
                b.line_to(pos, p);
                pos = p;
            }
            b'H' => {
                let p = [cur.number()? + origin[0], pos[1]];
                b.line_to(pos, p);
                pos = p;
            }
            b'V' => {
                let p = [pos[0], cur.number()? + origin[1]];
                b.line_to(pos, p);
                pos = p;
            }
            b'C' => {
                let c1 = add(origin, cur.pair()?);
                let c2 = add(origin, cur.pair()?);
                let p = add(origin, cur.pair()?);
                b.cubic_to(pos, c1, c2, p);
                next_cubic = Some(c2);
                pos = p;
            }
            b'S' => {
                let c1 = last_cubic.map_or(pos, |c| reflect(c, pos));
                let c2 = add(origin, cur.pair()?);
                let p = add(origin, cur.pair()?);
                b.cubic_to(pos, c1, c2, p);
                next_cubic = Some(c2);
                pos = p;
            }
            b'Q' => {
                let c = add(origin, cur.pair()?);
                let p = add(origin, cur.pair()?);
                b.quad_to(pos, c, p);
                next_quad = Some(c);
                pos = p;
            }
            b'T' => {
                let c = last_quad.map_or(pos, |q| reflect(q, pos));
                let p = add(origin, cur.pair()?);
                b.quad_to(pos, c, p);
                next_quad = Some(c);
                pos = p;
            }
            b'A' => {
                let rx = cur.number()?;
                let ry = cur.number()?;
                let x_axis_rotation_deg = cur.number()?;
                let large_arc = cur.flag()?;
                let sweep = cur.flag()?;
                let p = add(origin, cur.pair()?);
                let arc = ArcSeg {
                    rx,
                    ry,
                    x_axis_rotation_deg,
                    large_arc,
                    sweep,
                    to: p,
                };
                b.arc_to(pos, &arc);
                pos = p;
            }
            b'Z' => {
                b.flush();
                pos = start;
            }
            _ => {
                let found = char::from(cmd);
                return Err(parse_err(
                    cur.base + cur.pos - 1,
                    format!("unknown path command '{found}'"),
                ));
            }
        }
        last_cubic = next_cubic;
        last_quad = next_quad;
    }
    Ok(())
}

/// `points` of a polygon/polyline: coordinate pairs; an odd count is a
/// parse error.
fn parse_points(attr: &Attr) -> Result<Vec<[f64; 2]>, SvgError> {
    let mut cur = Cursor::new(attr);
    let mut points = Vec::new();
    loop {
        cur.skip_sep();
        if cur.peek().is_none() {
            break;
        }
        points.push(cur.pair()?);
    }
    Ok(points)
}

// ---------------------------------------------------------------------------
// Document walk
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FillRule {
    NonZero,
    EvenOdd,
}

/// The inherited presentation state.
#[derive(Debug, Clone, Copy)]
struct Style {
    painted: bool,
    rule: FillRule,
    ctm: Affine,
}

/// One polygon edge with `y0 < y1`; `dir` is the original winding
/// direction (+1 downward, -1 upward).
#[derive(Debug, Clone, Copy)]
struct Edge {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    dir: i32,
}

/// A fillable shape: its edges sorted by `y0`, and their y extent.
#[derive(Debug, Clone)]
struct Shape {
    rule: FillRule,
    edges: Vec<Edge>,
    y_min: f64,
    y_max: f64,
}

fn shape_from_polys(polys: &[Vec<[f64; 2]>], rule: FillRule) -> Option<Shape> {
    let mut edges = Vec::new();
    let (mut y_min, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for poly in polys {
        let next = poly.iter().cycle().skip(1);
        for (&[ax, ay], &[bx, by]) in poly.iter().zip(next) {
            let finite = ax.is_finite() && ay.is_finite() && bx.is_finite() && by.is_finite();
            if !finite || ay == by {
                continue;
            }
            let edge = if ay < by {
                Edge {
                    x0: ax,
                    y0: ay,
                    x1: bx,
                    y1: by,
                    dir: 1,
                }
            } else {
                Edge {
                    x0: bx,
                    y0: by,
                    x1: ax,
                    y1: ay,
                    dir: -1,
                }
            };
            y_min = y_min.min(edge.y0);
            y_max = y_max.max(edge.y1);
            edges.push(edge);
        }
    }
    if edges.is_empty() {
        return None;
    }
    edges.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    Some(Shape {
        rule,
        edges,
        y_min,
        y_max,
    })
}

/// The parsed document: raster size + device-space shapes.
struct Document {
    width: u32,
    height: u32,
    shapes: Vec<Shape>,
}

/// Applies an element's `fill`, `fill-rule` and `transform` over the
/// inherited style.
fn apply_presentation(parent: Style, attrs: &[Attr]) -> Result<Style, SvgError> {
    let mut style = parent;
    if let Some(fill) = find_attr(attrs, "fill") {
        match fill.value.trim() {
            "none" => style.painted = false,
            "" | "inherit" => {}
            _ => style.painted = true,
        }
    }
    if let Some(rule) = find_attr(attrs, "fill-rule") {
        match rule.value.trim() {
            "nonzero" => style.rule = FillRule::NonZero,
            "evenodd" => style.rule = FillRule::EvenOdd,
            _ => {}
        }
    }
    if let Some(transform) = find_attr(attrs, "transform") {
        style.ctm = style.ctm.compose(parse_transform(transform)?);
    }
    Ok(style)
}

/// A dimension in raster px: rounded, saturated at `u32::MAX`.
fn to_dimension(value: f64) -> u32 {
    let rounded = value.round();
    if rounded >= f64::from(u32::MAX) {
        u32::MAX
    } else if rounded > 0.0 {
        rounded as u32
    } else {
        0
    }
}

fn parse_viewbox(attr: &Attr) -> Result<[f64; 4], SvgError> {
    let mut cur = Cursor::new(attr);
    let vb = [cur.number()?, cur.number()?, cur.number()?, cur.number()?];
    cur.skip_ws();
    if cur.peek().is_some() {
        return Err(cur.error("trailing data after the four viewBox numbers"));
    }
    if vb[2] <= 0.0 || vb[3] <= 0.0 {
        return Err(parse_err(
            attr.value_offset,
            "viewBox width and height must be positive",
        ));
    }
    Ok(vb)
}

/// The root `svg`'s raster size and user → raster map.
fn root_geometry(attrs: &[Attr]) -> Result<(u32, u32, Affine), SvgError> {
    let root_length = |key: &str| -> Result<Option<f64>, SvgError> {
        let Some(attr) = find_attr(attrs, key) else {
            return Ok(None);
        };
        let value = parse_length(attr, "svg")?;
        if value < 0.0 {
            return Err(parse_err(
                attr.value_offset,
                format!("<svg> {key} must not be negative"),
            ));
        }
        Ok(Some(value))
    };
    let w = root_length("width")?;
    let h = root_length("height")?;
    let vb = find_attr(attrs, "viewBox").map(parse_viewbox).transpose()?;
    let (w, h) = match (w, h, vb) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, Some(vb)) => (w, w * vb[3] / vb[2]),
        (None, Some(h), Some(vb)) => (h * vb[2] / vb[3], h),
        (None, None, Some(vb)) => (vb[2], vb[3]),
        _ => return Err(SvgError::NoSize),
    };
    let (width, height) = (to_dimension(w), to_dimension(h));
    let ctm = match vb {
        Some([min_x, min_y, vb_w, vb_h]) => {
            let sx = f64::from(width) / vb_w;
            let sy = f64::from(height) / vb_h;
            Affine([sx, 0.0, 0.0, sy, -min_x * sx, -min_y * sy])
        }
        None => Affine::IDENTITY,
    };
    Ok((width, height, ctm))
}

/// Builds a shape element's device-space geometry; `None` when it
/// encloses nothing.
fn build_shape(name: &str, attrs: &[Attr], style: Style) -> Result<Option<Shape>, SvgError> {
    let mut b = PolyBuilder::new(style.ctm);
    match name {
        "path" => {
            if let Some(d) = find_attr(attrs, "d") {
                build_path(d, &mut b)?;
            }
        }
        "rect" => {
            let x = length_attr(attrs, "x", name)?.unwrap_or(0.0);
            let y = length_attr(attrs, "y", name)?.unwrap_or(0.0);
            let w = length_attr(attrs, "width", name)?.unwrap_or(0.0);
            let h = length_attr(attrs, "height", name)?.unwrap_or(0.0);
            if w <= 0.0 || h <= 0.0 {
                return Ok(None);
            }
            // A negative radius is invalid → auto; one radius given →
            // both; each clamps to half its side.
            let rx = length_attr(attrs, "rx", name)?.filter(|r| *r >= 0.0);
            let ry = length_attr(attrs, "ry", name)?.filter(|r| *r >= 0.0);
            let (rx, ry) = match (rx, ry) {
                (Some(rx), Some(ry)) => (rx, ry),
                (Some(r), None) | (None, Some(r)) => (r, r),
                (None, None) => (0.0, 0.0),
            };
            let (rx, ry) = (rx.min(w / 2.0), ry.min(h / 2.0));
            if rx > 0.0 && ry > 0.0 {
                let ring = [
                    [x + rx, y],
                    [x + w - rx, y],
                    [x + w, y + ry],
                    [x + w, y + h - ry],
                    [x + w - rx, y + h],
                    [x + rx, y + h],
                    [x, y + h - ry],
                    [x, y + ry],
                ];
                let mut from = ring[0];
                b.move_to(from);
                for (i, &to) in ring.iter().cycle().skip(1).take(ring.len()).enumerate() {
                    if i % 2 == 0 {
                        b.line_to(from, to);
                    } else {
                        let corner = ArcSeg {
                            rx,
                            ry,
                            x_axis_rotation_deg: 0.0,
                            large_arc: false,
                            sweep: true,
                            to,
                        };
                        b.arc_to(from, &corner);
                    }
                    from = to;
                }
            } else {
                b.move_to([x, y]);
                b.line_to([x, y], [x + w, y]);
                b.line_to([x + w, y], [x + w, y + h]);
                b.line_to([x + w, y + h], [x, y + h]);
            }
        }
        "circle" => {
            let cx = length_attr(attrs, "cx", name)?.unwrap_or(0.0);
            let cy = length_attr(attrs, "cy", name)?.unwrap_or(0.0);
            let r = length_attr(attrs, "r", name)?.unwrap_or(0.0);
            if r <= 0.0 {
                return Ok(None);
            }
            b.ellipse([cx, cy], r, r);
        }
        "ellipse" => {
            let cx = length_attr(attrs, "cx", name)?.unwrap_or(0.0);
            let cy = length_attr(attrs, "cy", name)?.unwrap_or(0.0);
            let rx = length_attr(attrs, "rx", name)?.unwrap_or(0.0);
            let ry = length_attr(attrs, "ry", name)?.unwrap_or(0.0);
            if rx <= 0.0 || ry <= 0.0 {
                return Ok(None);
            }
            b.ellipse([cx, cy], rx, ry);
        }
        "polygon" | "polyline" => {
            if let Some(points) = find_attr(attrs, "points") {
                let points = parse_points(points)?;
                if let Some((&first, rest)) = points.split_first() {
                    b.move_to(first);
                    let mut from = first;
                    for &to in rest {
                        b.line_to(from, to);
                        from = to;
                    }
                }
            }
        }
        "line" => {
            // Fill-only coverage: a line encloses no area. Its lengths
            // are still validated so a bad unit surfaces.
            for key in ["x1", "y1", "x2", "y2"] {
                let _ = length_attr(attrs, key, name)?;
            }
            return Ok(None);
        }
        _ => return Ok(None),
    }
    let polys = b.finish();
    // The hostile-input budget: one shape's flattened geometry is
    // bounded before the edge builder touches it.
    let points: usize = polys.iter().map(|p| p.len()).sum();
    if points > MAX_POINTS_PER_SHAPE {
        return Err(SvgError::TooComplex {
            what: "path points",
            limit: MAX_POINTS_PER_SHAPE,
        });
    }
    Ok(shape_from_polys(&polys, style.rule))
}

fn is_shape(name: &str) -> bool {
    matches!(
        name,
        "path" | "rect" | "circle" | "ellipse" | "polygon" | "polyline" | "line"
    )
}

/// Tokenizes and walks the document; the raster cap is checked at the
/// root tag, before any child is parsed.
fn parse_document(text: &str, base: usize, max_pixels: usize) -> Result<Document, SvgError> {
    let mut tok = Tokenizer::new(text, base);
    let (root_attrs, root_self_closing) = match tok.next_event()? {
        Event::Start {
            name,
            attrs,
            self_closing,
            offset,
        } => {
            if name != "svg" {
                return Err(parse_err(
                    offset,
                    format!("root element is <{name}>, expected <svg>"),
                ));
            }
            (attrs, self_closing)
        }
        Event::End | Event::Eof => return Err(parse_err(base, "no <svg> root element")),
    };
    let (width, height, root_ctm) = root_geometry(&root_attrs)?;
    if width == 0 || height == 0 {
        return Err(SvgError::ZeroSize { width, height });
    }
    if u64::from(width) * u64::from(height) > max_pixels as u64 {
        return Err(SvgError::TooLarge {
            width,
            height,
            max_pixels,
        });
    }
    let root_style = apply_presentation(
        Style {
            painted: true,
            rule: FillRule::NonZero,
            ctm: root_ctm,
        },
        &root_attrs,
    )?;
    // One frame per open element: `Some` = a container whose children
    // render under that style, `None` = a skipped subtree.
    let mut stack: Vec<Option<Style>> = Vec::new();
    if !root_self_closing {
        stack.push(Some(root_style));
    }
    let mut shapes = Vec::new();
    loop {
        match tok.next_event()? {
            Event::Eof => break,
            Event::End => {
                stack.pop();
            }
            Event::Start {
                name,
                attrs,
                self_closing,
                ..
            } => {
                let parent = stack.last().copied().flatten();
                let frame = match parent {
                    None => None,
                    Some(parent) if name == "g" => Some(apply_presentation(parent, &attrs)?),
                    Some(parent) if is_shape(&name) => {
                        let style = apply_presentation(parent, &attrs)?;
                        if style.painted {
                            shapes.extend(build_shape(&name, &attrs, style)?);
                            // The document-level shape budget (see
                            // [`MAX_SHAPES`]): checked as the document is
                            // walked, so a shape flood never reaches the
                            // edge builder or the rasterizer.
                            if shapes.len() > MAX_SHAPES {
                                return Err(SvgError::TooComplex {
                                    what: "shapes",
                                    limit: MAX_SHAPES,
                                });
                            }
                        }
                        None
                    }
                    // Unknown element: skip it and its subtree.
                    Some(_) => None,
                };
                if !self_closing {
                    stack.push(frame);
                }
            }
        }
    }
    Ok(Document {
        width,
        height,
        shapes,
    })
}

// ---------------------------------------------------------------------------
// Scanline rasterizer
// ---------------------------------------------------------------------------

/// `round(255 * hits / 16)`, half up.
fn coverage_byte(hits: u8) -> u8 {
    ((255 * u32::from(hits) + SUBSAMPLES / 2) / SUBSAMPLES) as u8
}

/// Marks the subsample columns whose centers lie in `[x0, x1)`; returns
/// whether any were marked.
fn fill_span(mask: &mut [bool], x0: f64, x1: f64) -> bool {
    let n = mask.len() as f64;
    let s = SUPERSAMPLE as f64;
    // Column k's center is (k + 0.5) / s, so the first column at or past
    // x is ceil(x * s - 0.5). NaN clamps through to 0 (`as` saturates).
    let start = (x0 * s - 0.5).ceil().clamp(0.0, n) as usize;
    let end = (x1 * s - 0.5).ceil().clamp(0.0, n) as usize;
    match mask.get_mut(start..end) {
        Some(span) if !span.is_empty() => {
            span.fill(true);
            true
        }
        _ => false,
    }
}

/// Rasterizes `shapes` (device space) into a `width * height` coverage
/// plane. Streams one pixel row at a time: a row of subsample bits for
/// the current subsample line, a row of per-pixel hit counts.
fn rasterize(width: u32, height: u32, shapes: &[Shape]) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut coverage = vec![0u8; w.saturating_mul(h)];
    if w == 0 || h == 0 || shapes.is_empty() {
        return coverage;
    }
    let mut mask = vec![false; w.saturating_mul(SUPERSAMPLE)];
    let mut hits = vec![0u8; w];
    let mut crossings: Vec<(f64, i32)> = Vec::new();
    let pitch = 1.0 / SUPERSAMPLE as f64;
    for (py, row) in coverage.chunks_exact_mut(w).enumerate() {
        let top = py as f64;
        if !shapes.iter().any(|s| s.y_max > top && s.y_min < top + 1.0) {
            continue;
        }
        hits.fill(0);
        for sy in 0..SUPERSAMPLE {
            let y = top + (sy as f64 + 0.5) * pitch;
            let mut touched = false;
            for shape in shapes {
                if !(shape.y_min..shape.y_max).contains(&y) {
                    continue;
                }
                crossings.clear();
                for e in &shape.edges {
                    if e.y0 > y {
                        break;
                    }
                    if y < e.y1 {
                        let t = (y - e.y0) / (e.y1 - e.y0);
                        crossings.push((e.x0 + t * (e.x1 - e.x0), e.dir));
                    }
                }
                crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut winding = 0i32;
                for pair in crossings.windows(2) {
                    winding += pair[0].1;
                    let inside = match shape.rule {
                        FillRule::NonZero => winding != 0,
                        FillRule::EvenOdd => winding % 2 != 0,
                    };
                    if inside {
                        touched |= fill_span(&mut mask, pair[0].0, pair[1].0);
                    }
                }
            }
            if touched {
                for (k, bit) in mask.iter_mut().enumerate() {
                    if *bit {
                        *bit = false;
                        if let Some(hit) = hits.get_mut(k / SUPERSAMPLE) {
                            *hit += 1;
                        }
                    }
                }
            }
        }
        for (out, &hit) in row.iter_mut().zip(&hits) {
            *out = coverage_byte(hit);
        }
    }
    coverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stencil::StencilImage;
    use std::f64::consts::PI;

    fn raster(src: &str) -> SvgRaster {
        rasterize_svg_default(src.as_bytes()).unwrap()
    }

    fn texel(r: &SvgRaster, x: u32, y: u32) -> u8 {
        r.coverage()[(y * r.width() + x) as usize]
    }

    fn parse_offset(err: SvgError) -> usize {
        match err {
            SvgError::Parse { offset, .. } => offset,
            other => panic!("expected a Parse error, got {other:?}"),
        }
    }

    const SCENE: &str = r#"<?xml version="1.0"?>
<!-- rect + half-px rect + quarter-px rect + circle + cubic hump -->
<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
  <rect x="2" y="2" width="8" height="6"/>
  <rect x="12.5" y="2" width="4" height="4"/>
  <rect x="12" y="10.75" width="4" height="2"/>
  <circle cx="24" cy="8" r="5"/>
  <path d="M 2 20 C 2 12, 14 12, 14 20 Z"/>
</svg>"#;

    // 1. rect + circle + cubic path → exact coverage at known texels.
    #[test]
    fn scene_pins_exact_coverage() {
        let r = raster(SCENE);
        assert_eq!((r.width(), r.height()), (32, 32));
        assert_eq!(r.coverage().len(), 32 * 32);
        // Raster corners: nothing there.
        assert_eq!(texel(&r, 0, 0), 0);
        assert_eq!(texel(&r, 31, 31), 0);
        // The 8x6 rect at (2,2): its corner texels inside, neighbours out.
        assert_eq!(texel(&r, 2, 2), 255);
        assert_eq!(texel(&r, 9, 7), 255);
        assert_eq!(texel(&r, 1, 2), 0);
        assert_eq!(texel(&r, 2, 1), 0);
        assert_eq!(texel(&r, 10, 7), 0);
        assert_eq!(texel(&r, 9, 8), 0);
        // x = 12.5 .. 16.5: half-covered edge pixels, 8 of 16 subsamples
        // → round(255 * 8 / 16) = round(127.5) = 128.
        assert_eq!(texel(&r, 12, 3), 128);
        assert_eq!(texel(&r, 13, 3), 255);
        assert_eq!(texel(&r, 16, 3), 128);
        assert_eq!(texel(&r, 17, 3), 0);
        // y = 10.75 .. 12.75: 1 of 4 subrows (4 hits) → 63.75 → 64;
        // 3 of 4 subrows (12 hits) → 191.25 → 191.
        assert_eq!(texel(&r, 13, 10), 64);
        assert_eq!(texel(&r, 13, 11), 255);
        assert_eq!(texel(&r, 13, 12), 191);
        // Circle (24,8) r5: texels whose every subsample is >= 0.05 px
        // inside (the flatness bound) are full; texels wholly beyond r
        // are empty (the polygon is inscribed).
        assert_eq!(texel(&r, 24, 8), 255);
        assert_eq!(texel(&r, 24, 4), 255);
        assert_eq!(texel(&r, 27, 8), 255);
        assert_eq!(texel(&r, 24, 2), 0);
        assert_eq!(texel(&r, 29, 8), 0);
        // Cubic hump: peak y = 14 at x = 8 (t = 0.5), chord y = 20.
        assert_eq!(texel(&r, 8, 19), 255);
        assert_eq!(texel(&r, 8, 16), 255);
        assert_eq!(texel(&r, 8, 20), 0);
        assert_eq!(texel(&r, 8, 13), 0);
        assert_eq!(texel(&r, 1, 19), 0);
    }

    // 2. evenodd vs nonzero on a self-intersecting pentagram.
    #[test]
    fn evenodd_and_nonzero_differ_on_a_star() {
        let star = |rule: &str| {
            raster(&format!(
                r#"<svg width="100" height="100"><path fill-rule="{rule}" d="M 50 5 L 79 95 L 2 39 L 98 39 L 21 95 Z"/></svg>"#
            ))
        };
        let nonzero = star("nonzero");
        let evenodd = star("evenodd");
        // The inner pentagon (winding 2): filled under nonzero only.
        assert_eq!(texel(&nonzero, 50, 50), 255);
        assert_eq!(texel(&evenodd, 50, 50), 0);
        // A star tip (winding 1): filled under both.
        assert_eq!(texel(&nonzero, 50, 10), 255);
        assert_eq!(texel(&evenodd, 50, 10), 255);
        // Default (no attribute) is nonzero.
        let default = raster(
            r#"<svg width="100" height="100"><path d="M 50 5 L 79 95 L 2 39 L 98 39 L 21 95 Z"/></svg>"#,
        );
        assert_eq!(default, nonzero);
        // Inherited from a <g>.
        let inherited = raster(
            r#"<svg width="100" height="100"><g fill-rule="evenodd"><path d="M 50 5 L 79 95 L 2 39 L 98 39 L 21 95 Z"/></g></svg>"#,
        );
        assert_eq!(inherited, evenodd);
    }

    // 3. Arc command vs its manual segment fan, within 1/255.
    #[test]
    fn arc_matches_its_manual_segment_fan() {
        let arc = raster(
            r#"<svg width="64" height="64"><path d="M 10 30 A 20 20 0 0 1 50 30 Z"/></svg>"#,
        );
        // Derived by hand: the endpoints are a diameter apart, so the
        // center is their midpoint (30, 30); sweep=1 runs the angle from
        // π (the start, left of center) up to 2π through 3π/2 (the top,
        // y-down) — the upper half disk.
        let n = arc_segment_count(20.0, PI);
        let (cx, cy, radius, theta1, dtheta) = (30.0, 30.0, 20.0, PI, PI);
        let mut points = vec!["10,30".to_string()];
        for i in 1..n {
            let theta: f64 = theta1 + dtheta * (i as f64 / n as f64);
            let (sin, cos) = theta.sin_cos();
            let (x, y) = (cx + radius * cos, cy + radius * sin);
            points.push(format!("{x},{y}"));
        }
        points.push("50,30".to_string());
        let fan = raster(&format!(
            r#"<svg width="64" height="64"><polygon points="{}"/></svg>"#,
            points.join(" ")
        ));
        let max_diff = arc
            .coverage()
            .iter()
            .zip(fan.coverage())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max_diff <= 1, "arc vs fan differ by {max_diff}/255");
        // And it IS the upper half: top of the disk in, below center out.
        assert_eq!(texel(&arc, 30, 15), 255);
        assert_eq!(texel(&arc, 30, 31), 0);
        assert_eq!(texel(&arc, 30, 8), 0);
        // The sweep=0 arc is the lower half instead.
        let lower = raster(
            r#"<svg width="64" height="64"><path d="M 10 30 A 20 20 0 0 0 50 30 Z"/></svg>"#,
        );
        assert_eq!(texel(&lower, 30, 15), 0);
        assert_eq!(texel(&lower, 30, 40), 255);
    }

    // 4. Over-cap refuses before allocating.
    #[test]
    fn over_cap_refuses_before_allocating() {
        // 10^10 px: allocating first would be a 10 GB plane.
        let src =
            r#"<svg width="100000" height="100000"><rect width="100000" height="100000"/></svg>"#;
        let expected = SvgError::TooLarge {
            width: 100_000,
            height: 100_000,
            max_pixels: 2048 * 2048,
        };
        assert_eq!(
            rasterize_svg(src.as_bytes(), 2048 * 2048),
            Err(expected.clone())
        );
        assert_eq!(rasterize_svg_default(src.as_bytes()), Err(expected));
        assert_eq!(DEFAULT_SVG_RASTER_PIXELS, 4_194_304);
        // The cap is inclusive: exactly max_pixels passes, one less refuses.
        let small = br#"<svg width="64" height="64"/>"#;
        assert!(rasterize_svg(small, 4096).is_ok());
        assert_eq!(
            rasterize_svg(small, 4095),
            Err(SvgError::TooLarge {
                width: 64,
                height: 64,
                max_pixels: 4095
            })
        );
    }

    // 5. viewBox-only SVG: raster size = viewBox w/h, rect lands right.
    #[test]
    fn viewbox_only_sizes_and_maps_the_raster() {
        let r = raster(
            r#"<svg viewBox="10 20 40 30"><rect x="20" y="30" width="5" height="5"/></svg>"#,
        );
        assert_eq!((r.width(), r.height()), (40, 30));
        assert_eq!(texel(&r, 10, 10), 255);
        assert_eq!(texel(&r, 14, 14), 255);
        assert_eq!(texel(&r, 9, 10), 0);
        assert_eq!(texel(&r, 15, 10), 0);
        assert_eq!(texel(&r, 10, 15), 0);
        // With width/height the viewBox stretches onto the raster (2x).
        let scaled = raster(
            r#"<svg width="80" height="60" viewBox="10 20 40 30"><rect x="20" y="30" width="5" height="5"/></svg>"#,
        );
        assert_eq!((scaled.width(), scaled.height()), (80, 60));
        assert_eq!(texel(&scaled, 20, 20), 255);
        assert_eq!(texel(&scaled, 29, 29), 255);
        assert_eq!(texel(&scaled, 30, 29), 0);
        assert_eq!(texel(&scaled, 19, 20), 0);
        // One dimension + viewBox derives the other from the aspect.
        let derived = raster(r#"<svg width="20" viewBox="0 0 10 5"/>"#);
        assert_eq!((derived.width(), derived.height()), (20, 10));
    }

    // 6. to_rgba → StencilImage::from_alpha == the plane cut at 128.
    #[test]
    fn round_trips_through_the_png_stencil_gate() {
        let r = raster(SCENE);
        let rgba = r.to_rgba();
        assert_eq!(rgba.len(), r.coverage().len() * 4);
        let i = (3 * 32 + 12) * 4;
        assert_eq!(&rgba[i..i + 4], &[255, 255, 255, 128]);
        let stencil = StencilImage::from_alpha(r.width(), r.height(), &rgba).unwrap();
        // 128 mirrors stencil.rs's private ALPHA_CUT.
        let cut: Vec<u8> = r
            .coverage()
            .iter()
            .map(|&c| if c >= 128 { 255 } else { 0 })
            .collect();
        assert_eq!(stencil.coverage(), cut.as_slice());
        // The partial texels split on the cut: 128 → in, 64 → out, 191 → in.
        assert_eq!(stencil.coverage()[3 * 32 + 12], 255);
        assert_eq!(stencil.coverage()[10 * 32 + 13], 0);
        assert_eq!(stencil.coverage()[12 * 32 + 13], 255);
    }

    // 7a. Malformed XML → Parse with the offset.
    #[test]
    fn malformed_xml_reports_the_offset() {
        let mismatched = r#"<svg width="4" height="4"><rect></svg>"#;
        assert_eq!(mismatched.find("</svg>"), Some(32));
        let err = rasterize_svg_default(mismatched.as_bytes()).unwrap_err();
        assert_eq!(parse_offset(err), 32);
        let unterminated = r#"<svg width="4" height="4"><rect"#;
        let err = rasterize_svg_default(unterminated.as_bytes()).unwrap_err();
        assert_eq!(parse_offset(err), 26);
        let unclosed = r#"<svg width="4" height="4"><g>"#;
        let err = rasterize_svg_default(unclosed.as_bytes()).unwrap_err();
        assert_eq!(parse_offset(err), 26);
        let after_root = r#"<svg width="4" height="4"/><svg/>"#;
        let err = rasterize_svg_default(after_root.as_bytes()).unwrap_err();
        assert_eq!(parse_offset(err), 27);
        let dup = r#"<svg width="4" width="4"/>"#;
        let err = rasterize_svg_default(dup.as_bytes()).unwrap_err();
        assert_eq!(parse_offset(err), 22);
        // Bad path data points at the offending byte.
        let path = r#"<svg width="4" height="4"><path d="M0 0 L x"/></svg>"#;
        assert_eq!(path.as_bytes()[42], b'x');
        let err = rasterize_svg_default(path.as_bytes()).unwrap_err();
        assert_eq!(parse_offset(err), 42);
        // Not an SVG root; not UTF-8.
        let err = rasterize_svg_default(b"<html/>").unwrap_err();
        assert_eq!(parse_offset(err), 0);
        let err = rasterize_svg_default(b"<svg width=\"4\"\xff").unwrap_err();
        assert_eq!(parse_offset(err), 14);
    }

    // 7b. Unsupported-construct policy: unknown elements skip their
    // subtree; non-px units are Unsupported.
    #[test]
    fn unknown_elements_skip_and_units_are_unsupported() {
        let r = raster(
            r#"<svg width="8" height="8">
  <metadata><rect width="8" height="8"/></metadata>
  <defs><rect width="8" height="8"/></defs>
  <style>rect { fill: none }</style>
  <text x="0" y="4">hi &amp; bye</text>
  <rect x="2" y="2" width="2" height="2"><title>t<rect width="8" height="8"/></title></rect>
</svg>"#,
        );
        let mut expected = [0u8; 64];
        for (x, y) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
            expected[y * 8 + x] = 255;
        }
        assert_eq!(r.coverage(), &expected[..]);
        let mm = rasterize_svg_default(br#"<svg width="10mm" height="10mm"/>"#);
        assert!(matches!(mm, Err(SvgError::Unsupported { .. })));
        let pct = rasterize_svg_default(br#"<svg width="100%" height="10" viewBox="0 0 1 1"/>"#);
        assert!(matches!(pct, Err(SvgError::Unsupported { .. })));
        // px is accepted and equals unitless.
        let px = raster(r#"<svg width="5px" height=" 3px "/>"#);
        assert_eq!((px.width(), px.height()), (5, 3));
    }

    // 7c. Transform composition: translate then rotate about a point.
    #[test]
    fn transform_composes_left_to_right() {
        // rotate(90 4 4) maps (x, y) → (8 - y, x): the rect x∈[2,6],
        // y∈[0,2] becomes x∈[6,8], y∈[2,6]; translate(10 0) then moves it
        // to x∈[16,18]. (The reversed order would land at y∈[12,16].)
        let r = raster(
            r#"<svg width="24" height="24"><rect transform="translate(10 0) rotate(90 4 4)" x="2" y="0" width="4" height="2"/></svg>"#,
        );
        let covered: Vec<(usize, usize)> = (0..24 * 24)
            .filter(|&i| r.coverage()[i] != 0)
            .map(|i| (i % 24, i / 24))
            .collect();
        let expected: Vec<(usize, usize)> =
            (2..6).flat_map(|y| (16..18).map(move |x| (x, y))).collect();
        assert_eq!(covered, expected);
        assert!(covered
            .iter()
            .all(|&(x, y)| r.coverage()[y * 24 + x] == 255));
        // The same through a nested <g> chain.
        let nested = raster(
            r#"<svg width="24" height="24"><g transform="translate(10,0)"><g transform="rotate(90,4,4)"><rect x="2" y="0" width="4" height="2"/></g></g></svg>"#,
        );
        assert_eq!(nested, r);
        // matrix() + scale(): matrix(1 0 0 1 3 4) scale(2) puts a unit
        // rect at x∈[3,5], y∈[4,6].
        let m = raster(
            r#"<svg width="8" height="8"><rect transform="matrix(1 0 0 1 3 4) scale(2)" width="1" height="1"/></svg>"#,
        );
        assert_eq!(texel(&m, 3, 4), 255);
        assert_eq!(texel(&m, 4, 5), 255);
        assert_eq!(texel(&m, 5, 5), 0);
        assert_eq!(texel(&m, 2, 4), 0);
        let bad = rasterize_svg_default(
            br#"<svg width="4" height="4"><g transform="rotate(1 2)"/></svg>"#,
        );
        assert!(matches!(bad, Err(SvgError::Parse { .. })));
    }

    #[test]
    fn path_syntax_relative_implicit_and_scientific() {
        let a =
            raster(r#"<svg width="40" height="40"><path d="m1e1,10 20,0 0 2e1 -20 0z"/></svg>"#);
        let b = raster(r#"<svg width="40" height="40"><path d="M10 10 H30 V30 h-20 z"/></svg>"#);
        let c = raster(
            r#"<svg width="40" height="40"><rect x="10" y="10" width="20" height="20"/></svg>"#,
        );
        assert_eq!(a, c);
        assert_eq!(b, c);
        assert_eq!(texel(&a, 10, 10), 255);
        assert_eq!(texel(&a, 29, 29), 255);
        assert_eq!(texel(&a, 30, 30), 0);
        assert_eq!(texel(&a, 9, 10), 0);
        // A second subpath after z starts fresh; evenodd punches a hole.
        let holed = raster(
            r#"<svg width="40" height="40"><path fill-rule="evenodd" d="M10 10h20v20h-20z M15 15h10v10h-10z"/></svg>"#,
        );
        assert_eq!(texel(&holed, 12, 12), 255);
        assert_eq!(texel(&holed, 20, 20), 0);
        // Path data must start with a moveto.
        let bad = rasterize_svg_default(br#"<svg width="4" height="4"><path d="L1 1"/></svg>"#);
        assert!(matches!(bad, Err(SvgError::Parse { .. })));
    }

    #[test]
    fn smooth_curves_reflect_the_previous_control_point() {
        // T reflects (10,0) through (20,20) → (30,40).
        let t = raster(
            r#"<svg width="48" height="48"><path d="M 0 20 Q 10 0 20 20 T 40 20 Z"/></svg>"#,
        );
        let q = raster(
            r#"<svg width="48" height="48"><path d="M 0 20 Q 10 0 20 20 Q 30 40 40 20 Z"/></svg>"#,
        );
        assert_eq!(t, q);
        assert_eq!(texel(&t, 9, 14), 255);
        assert_eq!(texel(&t, 30, 26), 255);
        // S reflects (15,0) through (20,20) → (25,40).
        let s = raster(
            r#"<svg width="48" height="48"><path d="M0 20 C 5 0 15 0 20 20 S 35 40 40 20 Z"/></svg>"#,
        );
        let c = raster(
            r#"<svg width="48" height="48"><path d="M0 20 C 5 0 15 0 20 20 C 25 40 35 40 40 20 Z"/></svg>"#,
        );
        assert_eq!(s, c);
        assert_eq!(texel(&s, 10, 10), 255);
        assert_eq!(texel(&s, 30, 30), 255);
    }

    #[test]
    fn fill_none_and_zero_area_contribute_nothing() {
        let r = raster(
            r##"<svg width="8" height="8">
  <rect fill="none" width="8" height="8"/>
  <g fill="none"><rect width="8" height="8"/><rect fill="#123" x="1" y="1" width="1" height="1"/></g>
  <line x1="0" y1="0" x2="8" y2="8"/>
  <rect width="0" height="8"/>
  <circle cx="4" cy="4" r="0"/>
  <path d="M0 0 L8 8 L0 0 Z"/>
  <polyline points="0,0 8,0"/>
</svg>"##,
        );
        let mut expected = [0u8; 64];
        expected[8 + 1] = 255;
        assert_eq!(r.coverage(), &expected[..]);
    }

    #[test]
    fn rounded_rect_and_ellipse_shapes() {
        let r =
            raster(r#"<svg width="20" height="20"><rect width="20" height="20" rx="5"/></svg>"#);
        assert_eq!(texel(&r, 0, 0), 0);
        assert_eq!(texel(&r, 19, 19), 0);
        assert_eq!(texel(&r, 10, 0), 255);
        assert_eq!(texel(&r, 0, 10), 255);
        let e =
            raster(r#"<svg width="20" height="10"><ellipse cx="10" cy="5" rx="10" ry="5"/></svg>"#);
        assert_eq!(texel(&e, 10, 5), 255);
        assert_eq!(texel(&e, 1, 0), 0);
        assert_eq!(texel(&e, 2, 4), 255);
        let p = raster(r#"<svg width="10" height="10"><polygon points="0,0 10,0 10,10"/></svg>"#);
        // Upper-right triangle: (8,1) in, (1,8) out.
        assert_eq!(texel(&p, 8, 1), 255);
        assert_eq!(texel(&p, 1, 8), 0);
    }

    #[test]
    fn size_errors() {
        assert_eq!(rasterize_svg_default(b"<svg></svg>"), Err(SvgError::NoSize));
        assert_eq!(
            rasterize_svg_default(br#"<svg width="0" height="5"/>"#),
            Err(SvgError::ZeroSize {
                width: 0,
                height: 5
            })
        );
        let neg = rasterize_svg_default(br#"<svg viewBox="0 0 -1 4"/>"#);
        assert!(matches!(neg, Err(SvgError::Parse { .. })));
    }

    /// The hostile-input budgets: a shape flood and a point flood are
    /// both refused with the named error, before the geometry is built.
    #[test]
    fn complexity_budgets_refuse_shape_and_point_floods() {
        // A shape flood: one more shape than the budget allows.
        let mut doc = String::from(r#"<svg width="4" height="4">"#);
        for _ in 0..=MAX_SHAPES {
            doc.push_str(r#"<rect x="0" y="0" width="1" height="1"/>"#);
        }
        doc.push_str("</svg>");
        assert_eq!(
            rasterize_svg_default(doc.as_bytes()),
            Err(SvgError::TooComplex {
                what: "shapes",
                limit: MAX_SHAPES,
            })
        );
        // A point flood: one path with more points than the budget.
        let mut d = String::from("M 0 0");
        for i in 1..=MAX_POINTS_PER_SHAPE {
            d.push_str(&format!(" L {} {}", i % 4, i % 4));
        }
        let doc = format!(r#"<svg width="4" height="4"><path d="{d}"/></svg>"#);
        assert_eq!(
            rasterize_svg_default(doc.as_bytes()),
            Err(SvgError::TooComplex {
                what: "path points",
                limit: MAX_POINTS_PER_SHAPE,
            })
        );
    }

    #[test]
    fn entities_decode_and_reject() {
        assert_eq!(
            decode_entities("a&amp;b&#65;&#x42;&lt;&gt;&quot;&apos;", 0).unwrap(),
            "a&bAB<>\"'"
        );
        assert_eq!(
            parse_offset(decode_entities("ab&foo;", 10).unwrap_err()),
            12
        );
        assert!(decode_entities("&#xD800;", 0).is_err());
        assert!(decode_entities("&amp", 0).is_err());
        assert!(decode_entities("a<b", 0).is_err());
        // Decoded values reach the attribute parser.
        let r = raster(
            r#"<svg width="&#52;" height="4"><rect width="4" height="4" fill-rule="even&#x6F;dd"/></svg>"#,
        );
        assert_eq!(r.width(), 4);
        assert_eq!(texel(&r, 3, 3), 255);
    }

    #[test]
    fn segment_counts_follow_the_flatness_bound() {
        // r = 20, tol = 0.05: step = 2 acos(0.9975) ≈ 0.14144 → π/step ≈ 22.2.
        assert_eq!(arc_segment_count(20.0, PI), 23);
        assert_eq!(arc_segment_count(0.0, PI), 1);
        assert_eq!(arc_segment_count(f64::INFINITY, PI), MAX_SEGMENTS_PER_CURVE);
        assert_eq!(segment_count(f64::NAN), 1);
        assert_eq!(segment_count(1e9), MAX_SEGMENTS_PER_CURVE);
        assert_eq!(Affine::IDENTITY.max_scale(), 1.0);
        assert_eq!(Affine::scale(3.0, 2.0).max_scale(), 3.0);
    }
}
