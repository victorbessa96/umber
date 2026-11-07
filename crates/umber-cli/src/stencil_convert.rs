//! `umber-cli stencil` (row 268, leaf B): headless `.svg` → `.png`
//! stencil conversion.
//!
//! `umber-cli stencil <file.svg> [--size N] <out.png>` reads the SVG,
//! rasterizes it to an 8-bit coverage plane through
//! [`umber_core::svg::rasterize_svg`], and writes a PNG stencil: white
//! RGB with `alpha = coverage`, exactly the shape
//! [`umber_core::stencil::StencilImage::from_alpha`] consumes (the same
//! plane the app's PNG stencil loader builds).
//!
//! **Rasterize at import, no vector state.** The landed decision (row
//! 265/266) is that an imported `.svg` is behaviorally a PNG stencil of
//! the same pixels; nothing but the coverage plane crosses this
//! command's boundary.
//!
//! **`--size N` is the rasterizer's pixel cap, not a target size.** The
//! output dimensions come from the SVG's own `width`/`height`/`viewBox`
//! (see [`umber_core::svg`]); `N` bounds `width * height`, defaulting to
//! [`umber_core::svg::DEFAULT_SVG_RASTER_PIXELS`] (2048²). The hard
//! ceiling is [`MAX_SVG_RASTER_PIXELS`] = 8192²: the rasterizer's 4x4
//! supersampling budget grows quadratically, and a `--size` beyond the
//! ceiling is refused by name BEFORE anything is allocated.
//!
//! **The PNG is data, written [`Transfer::Linear`].** The stencil plane
//! carries coverage in alpha, so the bytes go out as-is; an
//! [`Transfer::Srgb`] write would put the transfer curve on the RGB
//! channels (white stays white, but the plane is not an sRGB image and
//! should not be tagged as one).
//!
//! Output contract (matching `import-asset`'s refusal shape):
//!
//! - success: one line of JSON on stdout,
//!   `{"width":W,"height":H,"out":"path"}`, exit 0;
//! - refusal: `stencil: refused: {e}` on stderr, exit 2;
//! - missing/malformed arguments: the usage line on stderr, exit 2.
//!
//! [`convert`] is the testable CLI body: it takes the argument list and
//! explicit writers, so a test can drive both exit paths without a
//! subprocess.

use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

use umber_core::svg::{rasterize_svg, SvgError, DEFAULT_SVG_RASTER_PIXELS, MAX_SVG_INPUT_BYTES};
use umber_export::png::{PngError, Transfer};

/// The hard `--size` ceiling: 8192² pixels. The rasterizer keeps one row
/// of subsample bits plus one row of hit counts next to the output
/// plane, and the output/PNG buffers are `width * height` bytes times
/// 4-5; 8192² is the outer bound this command will ever allocate.
pub const MAX_SVG_RASTER_PIXELS: usize = 8192 * 8192;

/// The one-line usage string (also printed on a missing-argument
/// refusal).
pub const USAGE: &str = "usage: umber-cli stencil <file.svg> [--size N] <out.png>";

/// Everything that can refuse a conversion.
#[derive(Debug, Error)]
pub enum StencilConvertError {
    /// Missing or extra positional arguments, or an unknown flag.
    #[error("{0}")]
    Usage(&'static str),
    /// `--size` is not a positive whole number of pixels.
    #[error("--size '{value}': expected a positive whole number of pixels")]
    BadSize {
        /// The value as typed.
        value: String,
    },
    /// `--size` exceeds [`MAX_SVG_RASTER_PIXELS`].
    #[error(
        "--size {requested} exceeds the stencil ceiling of {ceiling} pixels (8192x8192); \
         the supersampling budget grows quadratically"
    )]
    TooLarge {
        /// The requested pixel cap.
        requested: usize,
        /// [`MAX_SVG_RASTER_PIXELS`].
        ceiling: usize,
    },
    /// The input SVG could not be read.
    #[error("reading {path}: {cause}")]
    Read {
        /// The input path.
        path: PathBuf,
        /// The OS error.
        cause: std::io::Error,
    },
    /// The SVG rasterizer refused the document.
    #[error(transparent)]
    Svg(#[from] SvgError),
    /// The PNG write failed.
    #[error(transparent)]
    Png(#[from] PngError),
    /// The stdout summary could not be serialized or written.
    #[error("writing the summary: {cause}")]
    Summary {
        /// The serialization/IO error.
        cause: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// The stdout summary: one JSON line.
#[derive(Debug, Serialize)]
struct Summary {
    width: u32,
    height: u32,
    out: String,
}

/// The parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    input: PathBuf,
    output: PathBuf,
    /// The rasterizer's pixel cap (`--size`, else the default).
    size: usize,
}

/// Runs `umber-cli stencil` and returns the process exit code: 0 on
/// success, 2 on any refusal. Success writes one JSON line to `out`;
/// refusals write to `err` (`stencil: refused: {e}`, or the bare usage
/// line for a usage error).
pub fn convert(args: &[String], out: &mut impl Write, err: &mut impl Write) -> i32 {
    match run(args, out) {
        Ok(()) => 0,
        Err(StencilConvertError::Usage(usage)) => {
            let _ = writeln!(err, "{usage}");
            2
        }
        Err(e) => {
            let _ = writeln!(err, "stencil: refused: {e}");
            2
        }
    }
}

/// [`convert`]'s body: parse, rasterize, write the PNG, print the
/// summary.
fn run(args: &[String], out: &mut impl Write) -> Result<(), StencilConvertError> {
    let Args {
        input,
        output,
        size,
    } = parse_args(args)?;
    // The input-byte budget (cross-review F2): refuse an oversized file
    // from its metadata, BEFORE the read — a 10 GB SVG must never be
    // pulled into memory just to be refused afterwards.
    let meta = std::fs::metadata(&input).map_err(|cause| StencilConvertError::Read {
        path: input.clone(),
        cause,
    })?;
    if meta.len() > MAX_SVG_INPUT_BYTES as u64 {
        return Err(StencilConvertError::Svg(SvgError::InputTooLarge {
            bytes: meta.len() as usize,
            max_bytes: MAX_SVG_INPUT_BYTES,
        }));
    }
    let bytes = std::fs::read(&input).map_err(|cause| StencilConvertError::Read {
        path: input.clone(),
        cause,
    })?;
    let raster = rasterize_svg(&bytes, size)?;
    let rgba = raster.to_rgba();
    // Linear: the stencil plane is data (white RGB, alpha = coverage),
    // not an sRGB image. See the module docs.
    umber_export::png::write_png(
        &output,
        raster.width(),
        raster.height(),
        &rgba,
        Transfer::Linear,
    )?;
    let summary = Summary {
        width: raster.width(),
        height: raster.height(),
        out: output.to_string_lossy().into_owned(),
    };
    let line = serde_json::to_string(&summary).map_err(|cause| StencilConvertError::Summary {
        cause: Box::new(cause),
    })?;
    writeln!(out, "{line}").map_err(|cause| StencilConvertError::Summary {
        cause: Box::new(cause),
    })?;
    Ok(())
}

/// Parses `<in.svg> [--size N] <out.png>`. Positionals are assigned in
/// order (input first, output second); `--size` may appear anywhere.
fn parse_args(args: &[String]) -> Result<Args, StencilConvertError> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut size: Option<usize> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--size" => {
                let value = args.get(i + 1).ok_or(StencilConvertError::Usage(USAGE))?;
                size = Some(parse_size(value)?);
                i += 2;
            }
            other if other.starts_with("--") => {
                return Err(StencilConvertError::Usage(USAGE));
            }
            other => {
                if input.is_none() {
                    input = Some(PathBuf::from(other));
                } else if output.is_none() {
                    output = Some(PathBuf::from(other));
                } else {
                    return Err(StencilConvertError::Usage(USAGE));
                }
                i += 1;
            }
        }
    }
    let input = input.ok_or(StencilConvertError::Usage(USAGE))?;
    let output = output.ok_or(StencilConvertError::Usage(USAGE))?;
    Ok(Args {
        input,
        output,
        size: size.unwrap_or(DEFAULT_SVG_RASTER_PIXELS),
    })
}

/// Parses and bounds one `--size` value: a positive pixel cap at most
/// [`MAX_SVG_RASTER_PIXELS`].
fn parse_size(value: &str) -> Result<usize, StencilConvertError> {
    let requested: usize = value.parse().map_err(|_| StencilConvertError::BadSize {
        value: value.to_string(),
    })?;
    if requested == 0 {
        return Err(StencilConvertError::BadSize {
            value: value.to_string(),
        });
    }
    if requested > MAX_SVG_RASTER_PIXELS {
        return Err(StencilConvertError::TooLarge {
            requested,
            ceiling: MAX_SVG_RASTER_PIXELS,
        });
    }
    Ok(requested)
}

#[cfg(test)]
mod tests {
    use super::*;

    use umber_core::stencil::StencilImage;

    /// A small document with a full-coverage rect and an anti-aliased
    /// circle (partial coverage exercises the 128 threshold).
    const SVG: &str = concat!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8">"#,
        r#"<rect x="1" y="1" width="4" height="4"/>"#,
        r#"<circle cx="5" cy="5" r="2"/>"#,
        r#"</svg>"#,
    );

    /// A fresh per-test scratch dir under the system temp dir.
    fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("umber-cli-stencil-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn converts_svg_to_png_matching_the_direct_raster() {
        let dir = tmp_dir("convert");
        let svg = dir.join("shape.svg");
        std::fs::write(&svg, SVG).unwrap();
        let png = dir.join("shape.png");

        let args = vec![svg.display().to_string(), png.display().to_string()];
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = convert(&args, &mut out, &mut err);
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));

        // One JSON line, the landed summary shape.
        let summary: serde_json::Value =
            serde_json::from_slice(&out).expect("stdout is one JSON value");
        assert_eq!(summary["width"].as_u64(), Some(8));
        assert_eq!(summary["height"].as_u64(), Some(8));
        assert_eq!(
            summary["out"].as_str(),
            Some(png.to_string_lossy().as_ref())
        );

        // Decode the written PNG and rebuild the app's stencil from it.
        let (w, h, rgba) = umber_export::png::read_png_rgba8(&png).expect("decode the PNG");
        assert_eq!((w, h), (8, 8));
        let stencil = StencilImage::from_alpha(w, h, &rgba).expect("rgba matches w*h*4");

        // The reference: the same bytes through the rasterizer directly,
        // thresholded at the loader's alpha cut (>= 128 covered).
        let direct = rasterize_svg(SVG.as_bytes(), DEFAULT_SVG_RASTER_PIXELS)
            .expect("the fixture rasterizes");
        assert_eq!((direct.width(), direct.height()), (w, h));
        // read_png_rgba8 returns GPU row order: output row 0 is the
        // file's LAST stored row (see its doc comment). The rasterizer
        // writes row 0 first, so decoded row y is source row h-1-y.
        for y in 0..h as usize {
            for x in 0..w as usize {
                let source = direct.coverage()[(h as usize - 1 - y) * w as usize + x];
                let want = if source >= 128 { 255 } else { 0 };
                assert_eq!(
                    stencil.coverage()[y * w as usize + x],
                    want,
                    "coverage at ({x}, {y}) (source alpha {source})"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn size_beyond_the_ceiling_refuses_without_writing() {
        let dir = tmp_dir("ceiling");
        let svg = dir.join("shape.svg");
        std::fs::write(&svg, SVG).unwrap();
        let png = dir.join("shape.png");

        let args = vec![
            svg.display().to_string(),
            "--size".to_string(),
            (MAX_SVG_RASTER_PIXELS + 1).to_string(),
            png.display().to_string(),
        ];
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = convert(&args, &mut out, &mut err);

        assert_eq!(code, 2, "an over-ceiling --size is a refusal");
        let stderr = String::from_utf8_lossy(&err);
        assert!(stderr.contains("stencil: refused:"), "{stderr}");
        assert!(stderr.contains("ceiling"), "{stderr}");
        assert!(out.is_empty(), "no summary on a refusal");
        assert!(!png.exists(), "refused before allocating or writing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn explicit_size_raises_the_raster_cap() {
        // 2049² = 4_198_401 pixels, just over the 2048² default cap: the
        // default run refuses, the --size run accepts. This proves the
        // flag reaches rasterize_svg (not just the parser).
        const SIDE: u32 = 2049;
        const PIXELS: u32 = SIDE * SIDE;
        let dir = tmp_dir("size-cap");
        let svg = dir.join("big.svg");
        std::fs::write(
            &svg,
            format!(r#"<svg width="{SIDE}" height="{SIDE}"><rect x="0" y="0" width="10" height="10"/></svg>"#),
        )
        .unwrap();
        let png = dir.join("big.png");

        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = convert(
            &[svg.display().to_string(), png.display().to_string()],
            &mut out,
            &mut err,
        );
        assert_eq!(code, 2, "the default 2048^2 cap refuses 2049^2");
        assert!(
            String::from_utf8_lossy(&err).contains("cap"),
            "{}",
            String::from_utf8_lossy(&err)
        );

        let code = convert(
            &[
                svg.display().to_string(),
                "--size".to_string(),
                PIXELS.to_string(),
                png.display().to_string(),
            ],
            &mut out,
            &mut err,
        );
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
        let (w, h, _) = umber_export::png::read_png_rgba8(&png).expect("decode the PNG");
        assert_eq!((w, h), (SIDE, SIDE));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_arguments_print_the_usage_and_exit_2() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(convert(&[], &mut out, &mut err), 2);
        assert!(
            String::from_utf8_lossy(&err).contains(USAGE),
            "{}",
            String::from_utf8_lossy(&err)
        );
        assert!(out.is_empty());

        // An input with no output is also a usage error.
        let mut err = Vec::new();
        assert_eq!(convert(&["in.svg".to_string()], &mut out, &mut err), 2);
        assert!(
            String::from_utf8_lossy(&err).contains(USAGE),
            "{}",
            String::from_utf8_lossy(&err)
        );

        // A third positional and an unknown flag are both refused.
        let mut err = Vec::new();
        assert_eq!(
            convert(
                &["a.svg".to_string(), "b.png".to_string(), "c".to_string()],
                &mut out,
                &mut err
            ),
            2
        );
        let mut err = Vec::new();
        assert_eq!(
            convert(
                &[
                    "a.svg".to_string(),
                    "--bogus".to_string(),
                    "b.png".to_string()
                ],
                &mut out,
                &mut err
            ),
            2
        );
    }

    #[test]
    fn size_parsing_accepts_up_to_the_ceiling() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        let ceiling = MAX_SVG_RASTER_PIXELS.to_string();
        assert_eq!(parse_size(&ceiling).unwrap(), MAX_SVG_RASTER_PIXELS);
        assert!(matches!(
            parse_size("0"),
            Err(StencilConvertError::BadSize { .. })
        ));
        assert!(matches!(
            parse_size("abc"),
            Err(StencilConvertError::BadSize { .. })
        ));
        let over = (MAX_SVG_RASTER_PIXELS + 1).to_string();
        assert!(matches!(
            parse_size(&over),
            Err(StencilConvertError::TooLarge { .. })
        ));
    }
}
