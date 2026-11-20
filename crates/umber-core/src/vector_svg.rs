//! Deterministic SVG export (K5a) and bounded SVG-to-vector import (K5b).
//!
//! Export uses a 1×1 UV viewBox, the same direct x/y coordinate mapping as
//! K2's `device_point`. K5b parsing delegates to `umber_core::svg`'s strict
//! tokenizer, root geometry, and path walker. Neither direction reads or
//! writes files itself; native file selection lives in the app panel.

use thiserror::Error;

use crate::vector::{VectorShape, MAX_VECTOR_ANCHORS};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VectorSvgError {
    #[error("vector has more than {limit} {what}")]
    TooComplex { what: &'static str, limit: usize },
    #[error("vector anchor {anchor} contains non-finite geometry")]
    NonFinite { anchor: usize },
    #[error("invalid stroke dash pattern")]
    InvalidDash,
}

/// Serializes one vector shape as a deterministic SVG 1.1 document. Open
/// paths never fill; closed paths use the shape's nonzero or even-odd fill
/// rule. Painted strokes carry the configured cap and K2's round join.
pub fn export_svg(shape: &VectorShape) -> Result<String, VectorSvgError> {
    if shape.anchors.len() > MAX_VECTOR_ANCHORS {
        return Err(VectorSvgError::TooComplex {
            what: "anchors",
            limit: MAX_VECTOR_ANCHORS,
        });
    }
    for (index, anchor) in shape.anchors.iter().enumerate() {
        if anchor
            .pos
            .iter()
            .chain(anchor.in_handle.iter())
            .chain(anchor.out_handle.iter())
            .any(|value| !value.is_finite())
        {
            return Err(VectorSvgError::NonFinite { anchor: index });
        }
    }

    let mut out = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\" viewBox=\"0 0 1 1\">\n<path d=\"",
    );
    if let Some(first) = shape.anchors.first() {
        out.push_str("M ");
        point(&mut out, first.pos);
        let segments = if shape.closed {
            shape.anchors.len()
        } else {
            shape.anchors.len().saturating_sub(1)
        };
        for index in 0..segments {
            let a = &shape.anchors[index];
            let b = &shape.anchors[(index + 1) % shape.anchors.len()];
            if a.out_handle == a.pos && b.in_handle == b.pos {
                out.push_str(" L ");
                point(&mut out, b.pos);
            } else {
                out.push_str(" C ");
                point(&mut out, a.out_handle);
                out.push(' ');
                point(&mut out, b.in_handle);
                out.push(' ');
                point(&mut out, b.pos);
            }
        }
        if shape.closed {
            out.push_str(" Z");
        }
    }
    out.push_str("\" fill-rule=\"");
    out.push_str(match shape.fill_rule {
        crate::svg::FillRule::NonZero => "nonzero",
        crate::svg::FillRule::EvenOdd => "evenodd",
    });
    out.push('"');

    let drawable = shape.anchors.len() >= 2;
    if drawable && shape.closed && shape.fill[3] > 0 {
        out.push_str(" fill=\"");
        out.push_str(&hex(shape.fill));
        out.push('"');
        if shape.fill[3] < 255 {
            out.push_str(" fill-opacity=\"");
            out.push_str(&alpha(shape.fill[3]));
            out.push('"');
        }
    } else {
        out.push_str(" fill=\"none\"");
    }

    let painted_stroke = shape.stroke.filter(|stroke| {
        drawable && stroke.color[3] > 0 && stroke.width.is_finite() && stroke.width > 0.0
    });
    if let Some(stroke) = painted_stroke {
        stroke
            .dash
            .validate()
            .map_err(|_| VectorSvgError::InvalidDash)?;
        out.push_str(" stroke=\"");
        out.push_str(&hex(stroke.color));
        out.push('"');
        if stroke.color[3] < 255 {
            out.push_str(" stroke-opacity=\"");
            out.push_str(&alpha(stroke.color[3]));
            out.push('"');
        }
        out.push_str(" stroke-width=\"");
        out.push_str(&number(stroke.width));
        out.push_str("\" stroke-linecap=\"");
        out.push_str(match stroke.cap {
            crate::vector::StrokeCap::Butt => "butt",
            crate::vector::StrokeCap::Round => "round",
            crate::vector::StrokeCap::Square => "square",
        });
        out.push_str("\" stroke-linejoin=\"");
        out.push_str(match stroke.join {
            crate::vector::StrokeJoin::Miter => "miter",
            crate::vector::StrokeJoin::Round => "round",
            crate::vector::StrokeJoin::Bevel => "bevel",
        });
        out.push('"');
        if !stroke.dash.is_solid() {
            out.push_str(" stroke-dasharray=\"");
            for (index, length) in stroke.dash.lengths().iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                out.push_str(&number(*length));
            }
            out.push('"');
            if stroke.dash.offset != 0.0 {
                out.push_str(" stroke-dashoffset=\"");
                out.push_str(&number(stroke.dash.offset));
                out.push('"');
            }
        }
    } else {
        out.push_str(" stroke=\"none\"");
    }
    out.push_str("/>\n</svg>\n");
    Ok(out)
}

/// Imports the bounded SVG subset into one vector shape per supported shape
/// or stroke-only subpath, with scoped group paint and affine transforms.
pub fn import_svg_many(
    bytes: &[u8],
) -> Result<Vec<crate::vector::VectorParams>, crate::svg::SvgError> {
    crate::svg::import_vector_svg_many(bytes)
}

/// Imports the bounded SVG subset when it contains exactly one vector shape.
/// Multi-shape documents remain available through [`import_svg_many`].
pub fn import_svg(bytes: &[u8]) -> Result<crate::vector::VectorParams, crate::svg::SvgError> {
    crate::svg::import_vector_svg(bytes)
}

fn hex(color: [u8; 4]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

fn alpha(value: u8) -> String {
    number(f32::from(value) / 255.0)
}

fn point(out: &mut String, value: [f32; 2]) {
    out.push_str(&number(value[0]));
    out.push(' ');
    out.push_str(&number(value[1]));
}

/// Rust's shortest f32 Display round-trips every finite value, including tiny
/// UV coordinates and stroke widths that fixed decimal places would lose.
/// Canonicalizing -0 keeps the document stable across equivalent shapes.
fn number(value: f32) -> String {
    let text = value.to_string();
    if text == "-0" {
        "0".to_owned()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::{Anchor, Stroke};

    #[test]
    fn closed_shape_matches_the_exact_deterministic_golden() {
        let shape = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 0, 128, 128],
            stroke: Some(Stroke {
                width: 0.01,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: crate::vector::StrokeDash::solid(),
                color: [0, 0, 0, 255],
            }),
            anchors: vec![
                Anchor::corner([0.0, -0.0]),
                Anchor {
                    pos: [0.5, 0.5],
                    in_handle: [0.25, 0.5],
                    out_handle: [0.75, 0.5],
                },
                Anchor::corner([1.0, 0.0]),
            ],
        };
        let actual = export_svg(&shape).expect("export");
        let expected = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\" viewBox=\"0 0 1 1\">\n",
            "<path d=\"M 0 0 C 0 0 0.25 0.5 0.5 0.5 C 0.75 0.5 1 0 1 0 L 0 0 Z\" ",
            "fill-rule=\"nonzero\" fill=\"#FF0080\" fill-opacity=\"0.5019608\" ",
            "stroke=\"#000000\" stroke-width=\"0.01\" stroke-linecap=\"round\" ",
            "stroke-linejoin=\"round\"/>\n</svg>\n"
        );
        assert_eq!(actual, expected);
        assert_eq!(export_svg(&shape).expect("second export"), actual);
    }

    #[test]
    fn open_and_unpainted_shapes_use_none_and_no_close() {
        let shape = VectorShape {
            anchors: vec![Anchor::corner([0.25, 0.25]), Anchor::corner([0.75, 0.75])],
            ..VectorShape::empty()
        };
        let actual = export_svg(&shape).expect("export");
        assert!(actual.contains("M 0.25 0.25 L 0.75 0.75"));
        assert!(!actual.contains(" Z"));
        assert!(actual.contains("fill=\"none\""));
        assert!(actual.contains("stroke=\"none\""));
    }

    #[test]
    fn fewer_than_two_anchors_export_no_paint() {
        let shape = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 255, 255, 255],
            stroke: Some(Stroke {
                width: 0.1,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: crate::vector::StrokeDash::solid(),
                color: [255, 255, 255, 255],
            }),
            anchors: vec![Anchor::corner([0.5, 0.5])],
        };
        let actual = export_svg(&shape).expect("export");
        assert!(actual.contains("fill=\"none\""));
        assert!(actual.contains("stroke=\"none\""));
    }

    #[test]
    fn invisible_or_invalid_strokes_are_stroke_none() {
        for stroke in [
            Stroke {
                width: 0.0,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: crate::vector::StrokeDash::solid(),
                color: [255, 255, 255, 255],
            },
            Stroke {
                width: f32::NAN,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: crate::vector::StrokeDash::solid(),
                color: [255, 255, 255, 255],
            },
            Stroke {
                width: 0.1,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: crate::vector::StrokeDash::solid(),
                color: [255, 255, 255, 0],
            },
        ] {
            let shape = VectorShape {
                stroke: Some(stroke),
                anchors: vec![Anchor::corner([0.0, 0.0]), Anchor::corner([1.0, 1.0])],
                ..VectorShape::empty()
            };
            assert!(export_svg(&shape)
                .expect("export")
                .contains("stroke=\"none\""));
        }
    }

    #[test]
    fn non_finite_geometry_and_anchor_floods_are_typed_refusals() {
        let mut bad = VectorShape::empty();
        bad.anchors.push(Anchor::corner([f32::INFINITY, 0.0]));
        assert_eq!(
            export_svg(&bad),
            Err(VectorSvgError::NonFinite { anchor: 0 })
        );
        let full = VectorShape {
            anchors: vec![Anchor::corner([0.5, 0.5]); MAX_VECTOR_ANCHORS + 1],
            ..VectorShape::empty()
        };
        assert_eq!(
            export_svg(&full),
            Err(VectorSvgError::TooComplex {
                what: "anchors",
                limit: MAX_VECTOR_ANCHORS,
            })
        );
    }

    #[test]
    fn number_format_roundtrips_small_f32_values() {
        for value in [
            f32::from_bits(0x3c08_9c92),
            0.0001,
            0.0000001,
            -f32::from_bits(0x3c08_9c92),
            f32::from(128u8) / 255.0,
        ] {
            let text = number(value);
            let parsed: f32 = text.parse().expect("formatted number parses");
            assert_eq!(parsed.to_bits(), value.to_bits(), "{value} -> {text}");
        }
        assert_eq!(number(-0.0), "0");
    }

    #[test]
    fn every_alpha_byte_roundtrips_through_svg_opacity() {
        for alpha in 0..=255u8 {
            let shape = VectorShape {
                closed: true,
                fill_rule: crate::svg::FillRule::NonZero,
                fill: [1, 2, 3, alpha],
                stroke: None,
                anchors: vec![Anchor::corner([0.0, 0.0]), Anchor::corner([1.0, 1.0])],
            };
            let svg = export_svg(&shape).expect("export");
            let imported = import_svg(svg.as_bytes()).expect("import");
            assert_eq!(imported.shape.fill[3], alpha, "fill alpha {alpha}");

            let shape = VectorShape {
                closed: true,
                fill_rule: crate::svg::FillRule::NonZero,
                fill: [0, 0, 0, 0],
                stroke: Some(Stroke {
                    width: 0.1,
                    cap: crate::vector::StrokeCap::Round,

                    join: crate::vector::StrokeJoin::Round,
                    dash: crate::vector::StrokeDash::solid(),
                    color: [1, 2, 3, alpha],
                }),
                anchors: vec![Anchor::corner([0.0, 0.0]), Anchor::corner([1.0, 1.0])],
            };
            let svg = export_svg(&shape).expect("stroke export");
            let imported = import_svg(svg.as_bytes()).expect("stroke import");
            if alpha == 0 {
                assert_eq!(imported.shape.stroke, None);
            } else {
                assert_eq!(
                    imported.shape.stroke.expect("painted stroke").color[3],
                    alpha,
                    "stroke alpha {alpha}"
                );
            }
        }
    }

    #[test]
    fn canonical_export_import_export_is_byte_identical() {
        let closed_mixed = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 0, 128, 128],
            stroke: Some(Stroke {
                width: 0.01,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: crate::vector::StrokeDash::solid(),
                color: [0, 0, 0, 255],
            }),
            anchors: vec![
                Anchor::corner([0.0, -0.0]),
                Anchor {
                    pos: [0.5, 0.5],
                    in_handle: [0.25, 0.5],
                    out_handle: [0.75, 0.5],
                },
                Anchor::corner([1.0, 0.0]),
            ],
        };
        let open_line = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [0, 0, 0, 0],
            stroke: Some(Stroke {
                width: 0.02,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: crate::vector::StrokeDash::solid(),
                color: [10, 20, 30, 255],
            }),
            anchors: vec![Anchor::corner([0.1, 0.2]), Anchor::corner([0.8, 0.9])],
        };
        let open_cubic = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [0, 0, 0, 0],
            stroke: None,
            anchors: vec![
                Anchor::corner([0.0, 0.0]),
                Anchor {
                    pos: [0.5, 0.5],
                    in_handle: [0.25, 0.5],
                    out_handle: [0.75, 0.5],
                },
                Anchor::corner([1.0, 0.0]),
            ],
        };
        let stroke_alpha = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [1, 2, 3, 255],
            stroke: Some(Stroke {
                width: 0.125,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: crate::vector::StrokeDash::solid(),
                color: [10, 20, 30, 128],
            }),
            anchors: vec![Anchor::corner([0.0, 0.0]), Anchor::corner([1.0, 1.0])],
        };
        for (name, shape) in [
            ("closed mixed", closed_mixed),
            ("open line", open_line),
            ("open cubic", open_cubic),
            ("stroke alpha", stroke_alpha),
        ] {
            let first = export_svg(&shape).expect("export");
            let imported = import_svg(first.as_bytes()).expect("import");
            assert_eq!(imported.shape, shape, "{name}");
            let second = export_svg(&imported.shape).expect("re-export");
            assert_eq!(second, first, "{name}");
        }
    }

    #[test]
    fn unpainted_fill_normalization_is_pinned() {
        let open_painted = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 0, 0, 255],
            stroke: None,
            anchors: vec![Anchor::corner([0.0, 0.0]), Anchor::corner([1.0, 1.0])],
        };
        let first = export_svg(&open_painted).expect("open export");
        let imported = import_svg(first.as_bytes()).expect("open import");
        assert_eq!(imported.shape.fill, [0, 0, 0, 0]);
        assert_eq!(export_svg(&imported.shape).expect("open re-export"), first);

        let closed_stroke_only = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [1, 2, 3, 0],
            stroke: Some(Stroke {
                width: 0.1,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: crate::vector::StrokeDash::solid(),
                color: [255, 255, 255, 255],
            }),
            anchors: vec![Anchor::corner([0.0, 0.0]), Anchor::corner([1.0, 1.0])],
        };
        let first = export_svg(&closed_stroke_only).expect("closed export");
        let imported = import_svg(first.as_bytes()).expect("closed import");
        assert_eq!(imported.shape.fill, [0, 0, 0, 0]);
        assert_eq!(
            export_svg(&imported.shape).expect("closed re-export"),
            first
        );
    }

    #[test]
    fn quadratic_and_smooth_quadratic_elevate_to_exact_cubics() {
        let svg =
            br#"<svg width="1" height="1"><path d="m 0.1 0.1 q 0.2 0.3 0.4 0.1 t 0.4 0"/></svg>"#;
        let params = import_svg(svg).expect("import");
        assert_eq!(params.shape.anchors.len(), 3);
        let anchors = &params.shape.anchors;
        let uv = |point: [f64; 2]| [point[0] as f32, point[1] as f32];
        let elevate = |from: [f64; 2], ctrl: [f64; 2], to: [f64; 2]| {
            (
                [
                    from[0] + (2.0 / 3.0) * (ctrl[0] - from[0]),
                    from[1] + (2.0 / 3.0) * (ctrl[1] - from[1]),
                ],
                [
                    to[0] + (2.0 / 3.0) * (ctrl[0] - to[0]),
                    to[1] + (2.0 / 3.0) * (ctrl[1] - to[1]),
                ],
            )
        };
        let (c1, c2) = elevate([0.1, 0.1], [0.3, 0.4], [0.5, 0.2]);
        assert_eq!(anchors[0].out_handle, uv(c1));
        assert_eq!(anchors[1].pos, uv([0.5, 0.2]));
        assert_eq!(anchors[1].in_handle, uv(c2));
        let reflected = [2.0 * 0.5 - 0.3, 2.0 * 0.2 - 0.4];
        let (c1, c2) = elevate([0.5, 0.2], reflected, [0.9, 0.2]);
        assert_eq!(anchors[1].out_handle, uv(c1));
        assert_eq!(anchors[2].in_handle, uv(c2));
        assert_eq!(anchors[2].pos, uv([0.9, 0.2]));
    }

    #[test]
    fn viewbox_short_hex_and_opacity_map_to_uv() {
        let svg = br##"<svg width="100" height="200" viewBox="0 0 10 20"><path d="M1 2 L3 4 Z" fill="#abc" fill-opacity="0.5"/></svg>"##;
        let params = import_svg(svg).expect("import");
        assert_eq!(params.shape.anchors[0].pos, [0.1, 0.1]);
        assert_eq!(params.shape.anchors[1].pos, [0.3, 0.2]);
        assert_eq!(params.shape.fill, [170, 187, 204, 128]);
        assert!(params.shape.closed);
        assert_eq!(params.shape.stroke, None);
    }

    #[test]
    fn root_group_and_path_transforms_compose_once_and_scale_strokes() {
        let svg =
            br#"<svg width="2" height="2" transform="translate(1 0)"><path d="M0 0 L1 1"/></svg>"#;
        let params = import_svg(svg).expect("one root transform");
        assert_eq!(params.shape.anchors[0].pos, [0.5, 0.0]);
        assert_eq!(params.shape.anchors[1].pos, [1.0, 0.5]);

        let nested = br##"<svg width="4" height="4"><g transform="translate(1 0)"><g transform="scale(2)"><path transform="translate(0.5 0.5)" fill="none" stroke="#000000" stroke-width="0.5" stroke-dasharray="1 2" stroke-dashoffset="0.5" d="M0 0 L1 0"/></g></g><path d="M0 0 L1 0"/></svg>"##;
        let shapes = import_svg_many(nested).expect("nested transforms");
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[0].shape.anchors[0].pos, [0.5, 0.25]);
        assert_eq!(shapes[0].shape.anchors[1].pos, [1.0, 0.25]);
        let stroke = shapes[0].shape.stroke.as_ref().expect("scaled stroke");
        assert_eq!(stroke.width, 0.25);
        assert_eq!(stroke.dash.lengths(), &[0.5, 1.0]);
        assert_eq!(stroke.dash.offset, 0.25);
        assert_eq!(shapes[1].shape.anchors[0].pos, [0.0, 0.0]);
        assert_eq!(shapes[1].shape.anchors[1].pos, [0.25, 0.0]);

        for invalid in [
            r#"<svg width="1" height="1"><g transform="bogus(1)"><path d="M0 0"/></g></svg>"#,
            r#"<svg width="1" height="1"><path transform="scale(1 2 3)" d="M0 0"/></svg>"#,
        ] {
            assert!(import_svg(invalid.as_bytes()).is_err());
        }
    }

    #[test]
    fn evenodd_exports_and_imports_as_the_same_rule() {
        let shape = VectorShape {
            closed: true,
            fill: [255, 0, 0, 255],
            fill_rule: crate::svg::FillRule::EvenOdd,
            stroke: None,
            anchors: vec![
                Anchor::corner([0.50, 0.05]),
                Anchor::corner([0.79, 0.95]),
                Anchor::corner([0.02, 0.39]),
                Anchor::corner([0.98, 0.39]),
                Anchor::corner([0.21, 0.95]),
            ],
        };
        let first = export_svg(&shape).expect("evenodd export");
        assert!(first.contains("fill-rule=\"evenodd\""));
        let imported = import_svg(first.as_bytes()).expect("evenodd import");
        assert_eq!(imported.shape.fill_rule, crate::svg::FillRule::EvenOdd);
        assert_eq!(export_svg(&imported.shape).expect("re-export"), first);

        let root =
            br#"<svg width="1" height="1" fill-rule="evenodd"><path d="M0 0 L1 1 Z"/></svg>"#;
        assert_eq!(
            import_svg(root)
                .expect("root evenodd import")
                .shape
                .fill_rule,
            crate::svg::FillRule::EvenOdd
        );
        let invalid = br#"<svg width="1" height="1" fill-rule="winding"><path d="M0 0"/></svg>"#;
        assert!(matches!(
            import_svg(invalid),
            Err(crate::svg::SvgError::Unsupported { .. })
        ));
    }

    #[test]
    fn stroke_caps_export_and_import_as_the_same_cap() {
        for (cap, text) in [
            (crate::vector::StrokeCap::Butt, "butt"),
            (crate::vector::StrokeCap::Round, "round"),
            (crate::vector::StrokeCap::Square, "square"),
        ] {
            let shape = VectorShape {
                closed: false,
                fill_rule: crate::svg::FillRule::NonZero,
                fill: [0, 0, 0, 0],
                stroke: Some(Stroke {
                    width: 0.25,
                    color: [10, 20, 30, 255],
                    cap,
                    join: crate::vector::StrokeJoin::Round,
                    dash: crate::vector::StrokeDash::solid(),
                }),
                anchors: vec![Anchor::corner([0.25, 0.5]), Anchor::corner([0.75, 0.5])],
            };
            let first = export_svg(&shape).expect("cap export");
            assert!(first.contains(&format!("stroke-linecap=\"{text}\"")));
            let imported = import_svg(first.as_bytes()).expect("cap import");
            assert_eq!(imported.shape.stroke.expect("imported stroke").cap, cap);
            assert_eq!(export_svg(&imported.shape).expect("re-export"), first);
        }
        let invalid = br##"<svg width="1" height="1"><path stroke="#000" stroke-linecap="pointed" d="M0 0"/></svg>"##;
        assert!(matches!(
            import_svg(invalid),
            Err(crate::svg::SvgError::Unsupported { .. })
        ));
    }

    #[test]
    fn stroke_joins_export_and_import_as_the_same_join() {
        for (join, text) in [
            (crate::vector::StrokeJoin::Miter, "miter"),
            (crate::vector::StrokeJoin::Round, "round"),
            (crate::vector::StrokeJoin::Bevel, "bevel"),
        ] {
            let shape = VectorShape {
                closed: false,
                fill_rule: crate::svg::FillRule::NonZero,
                fill: [0, 0, 0, 0],
                stroke: Some(Stroke {
                    width: 0.25,
                    color: [10, 20, 30, 255],
                    cap: crate::vector::StrokeCap::Butt,
                    join,
                    dash: crate::vector::StrokeDash::solid(),
                }),
                anchors: vec![Anchor::corner([0.25, 0.75]), Anchor::corner([0.50, 0.25])],
            };
            let first = export_svg(&shape).expect("join export");
            assert!(first.contains(&format!("stroke-linejoin=\"{text}\"")));
            let imported = import_svg(first.as_bytes()).expect("join import");
            assert_eq!(imported.shape.stroke.expect("imported stroke").join, join);
            assert_eq!(export_svg(&imported.shape).expect("re-export"), first);
        }
        let invalid = br##"<svg width="1" height="1"><path stroke="#000" stroke-linejoin="pointed" d="M0 0"/></svg>"##;
        assert!(matches!(
            import_svg(invalid),
            Err(crate::svg::SvgError::Unsupported { .. })
        ));
    }

    #[test]
    fn dash_patterns_export_import_and_reject_invalid_values() {
        let dash =
            crate::vector::StrokeDash::from_lengths(&[0.25, 0.125], 0.0625).expect("valid dash");
        let shape = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [0, 0, 0, 0],
            stroke: Some(Stroke {
                width: 0.25,
                color: [10, 20, 30, 255],
                cap: crate::vector::StrokeCap::Butt,
                join: crate::vector::StrokeJoin::Bevel,
                dash,
            }),
            anchors: vec![Anchor::corner([0.25, 0.75]), Anchor::corner([0.50, 0.25])],
        };
        let first = export_svg(&shape).expect("dash export");
        assert!(first.contains("stroke-dasharray=\"0.25 0.125\""));
        assert!(first.contains("stroke-dashoffset=\"0.0625\""));
        let imported = import_svg(first.as_bytes()).expect("dash import");
        assert_eq!(imported.shape.stroke.expect("stroke").dash, dash);
        assert_eq!(export_svg(&imported.shape).expect("re-export"), first);

        let odd = crate::vector::StrokeDash::from_lengths(&[0.25], -0.125).expect("odd dash");
        let mut odd_shape = shape.clone();
        odd_shape.stroke.as_mut().unwrap().dash = odd;
        let odd_svg = export_svg(&odd_shape).expect("odd export");
        let odd_import = import_svg(odd_svg.as_bytes()).expect("odd import");
        assert_eq!(odd_import.shape.stroke.expect("odd stroke").dash, odd);
        assert_eq!(
            export_svg(&odd_import.shape).expect("odd re-export"),
            odd_svg
        );

        for invalid in [
            "stroke-dasharray=\"0 0\"",
            "stroke-dasharray=\"-1 2\"",
            "stroke-dasharray=\"1 2 3 4 5 6 7 8 9\"",
        ] {
            let svg = format!(
                "<svg width=\"1\" height=\"1\"><path stroke=\"#000\" {invalid} d=\"M0 0 L1 1\"/></svg>"
            );
            assert!(matches!(
                import_svg(svg.as_bytes()),
                Err(crate::svg::SvgError::Unsupported { .. })
            ));
        }
        let solid = br##"<svg width="1" height="1"><path stroke="#000" stroke-dasharray="none" d="M0 0 L1 1"/></svg>"##;
        assert!(import_svg(solid)
            .expect("none dash")
            .shape
            .stroke
            .expect("stroke")
            .dash
            .is_solid());
    }

    #[test]
    fn multi_path_import_preserves_order_paint_and_single_path_compatibility() {
        let svg = br##"<svg width="1" height="1"><path fill="#FF0000" d="M0 0 L1 0 L1 1 Z"/><path fill="#00FF00" stroke="#000000" stroke-dasharray="0.25 0.25" d="M0 0 L1 1"/></svg>"##;
        let shapes = import_svg_many(svg).expect("two paths import");
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[0].shape.fill, [255, 0, 0, 255]);
        assert_eq!(shapes[1].shape.fill, [0, 255, 0, 255]);
        assert_eq!(
            shapes[1]
                .shape
                .stroke
                .expect("second path stroke")
                .dash
                .lengths(),
            &[0.25, 0.25]
        );
        assert!(matches!(
            import_svg(svg),
            Err(crate::svg::SvgError::Unsupported { .. })
        ));
    }

    #[test]
    fn basic_shapes_import_exact_geometry_and_closure() {
        let svg = br##"<svg width="8" height="8"><g transform="translate(1 0)" stroke="#000000"><rect x="1" y="2" width="3" height="4" rx="-1"/><polyline points="0,0 1,0 1,1" fill="none"/><polygon points="0 0 2 0 1 2"/><line x1="0" y1="7" x2="1" y2="7"/></g><rect width="0" height="1"/><polyline/></svg>"##;
        let shapes = import_svg_many(svg).expect("basic shapes");
        assert_eq!(shapes.len(), 4);
        let rect = &shapes[0].shape;
        assert!(rect.closed);
        assert_eq!(
            rect.anchors
                .iter()
                .map(|anchor| anchor.pos)
                .collect::<Vec<_>>(),
            vec![[0.25, 0.25], [0.625, 0.25], [0.625, 0.75], [0.25, 0.75]]
        );
        assert!(!shapes[1].shape.closed);
        assert_eq!(shapes[1].shape.anchors.len(), 3);
        assert_eq!(shapes[1].shape.fill, [0, 0, 0, 0]);
        assert!(shapes[2].shape.closed);
        assert_eq!(shapes[2].shape.anchors.len(), 3);
        assert!(!shapes[3].shape.closed);
        assert_eq!(shapes[3].shape.anchors[0].pos, [0.125, 0.875]);
        assert_eq!(shapes[3].shape.anchors[1].pos, [0.25, 0.875]);
        assert!(shapes[3].shape.stroke.is_some());

        let one = br#"<svg width="1" height="1"><rect width="1" height="1"/></svg>"#;
        let rect = import_svg(one).expect("single rect compatibility").shape;
        assert!(rect.closed);
        assert_eq!(rect.anchors.len(), 4);
        let square =
            br#"<svg width="1" height="1"><rect width="1" height="1" rx="1" ry="0"/></svg>"#;
        assert!(import_svg(square).is_ok());

        for unsupported in [
            r#"<svg width="1" height="1"><rect width="1" height="1" rx="0.5"/></svg>"#,
            r#"<svg width="1" height="1"><circle r="1"/></svg>"#,
            r#"<svg width="1" height="1"><ellipse rx="1" ry="1"/></svg>"#,
            r#"<svg width="1" height="1"><line x1="1mm"/></svg>"#,
        ] {
            assert!(matches!(
                import_svg(unsupported.as_bytes()),
                Err(crate::svg::SvgError::Unsupported { .. })
            ));
        }
    }

    #[test]
    fn stroke_only_subpaths_split_and_filled_compounds_are_refused() {
        let svg = br##"<svg width="1" height="1"><path fill="none" stroke="#000000" d="M0 0 L1 0 M0 1 L1 1"/></svg>"##;
        let shapes = import_svg_many(svg).expect("stroke subpaths split");
        assert_eq!(shapes.len(), 2);
        assert!(shapes.iter().all(|params| params.shape.fill[3] == 0));
        assert!(shapes.iter().all(|params| params.shape.stroke.is_some()));
        assert_eq!(shapes[0].shape.anchors[0].pos, [0.0, 0.0]);
        assert_eq!(shapes[1].shape.anchors[0].pos, [0.0, 1.0]);
        assert!(matches!(
            import_svg(svg),
            Err(crate::svg::SvgError::Unsupported { .. })
        ));

        let after_close = br##"<svg width="1" height="1"><path fill="none" stroke="#000000" d="M0 0 L1 0 Z L1 1"/></svg>"##;
        let split = import_svg_many(after_close).expect("segment after closepath splits");
        assert_eq!(split.len(), 2);
        assert!(split[0].shape.closed);
        assert!(!split[1].shape.closed);
        assert_eq!(split[1].shape.anchors[0].pos, [0.0, 0.0]);

        let filled = br##"<svg width="1" height="1"><path fill="#FFFFFF" d="M0 0 L1 0 L1 1 Z M0 0 L1 1 L0 1 Z"/></svg>"##;
        assert!(matches!(
            import_svg_many(filled),
            Err(crate::svg::SvgError::Unsupported { what }) if what.contains("filled compound")
        ));

        let mut d = String::new();
        for index in 0..=crate::vector::MAX_VECTOR_SHAPES {
            d.push_str(&format!("M{index} 0 L{index} 1 "));
        }
        let flood = format!(
            "<svg width=\"1\" height=\"1\"><path fill=\"none\" stroke=\"#000\" d=\"{d}\"/></svg>"
        );
        assert!(matches!(
            import_svg_many(flood.as_bytes()),
            Err(crate::svg::SvgError::TooComplex {
                what: "vector paths",
                ..
            })
        ));
    }

    #[test]
    fn multi_path_import_has_shape_and_total_anchor_budgets() {
        let mut accepted_paths = String::from("<svg width=\"1\" height=\"1\"><g>");
        for _ in 0..crate::vector::MAX_VECTOR_SHAPES {
            accepted_paths.push_str("<path d=\"M0 0 L1 1\"/>");
        }
        accepted_paths.push_str("</g></svg>");
        assert_eq!(
            import_svg_many(accepted_paths.as_bytes())
                .expect("exact path cap is accepted")
                .len(),
            crate::vector::MAX_VECTOR_SHAPES
        );

        let mut paths = String::from("<svg width=\"1\" height=\"1\"><g>");
        for _ in 0..=crate::vector::MAX_VECTOR_SHAPES {
            paths.push_str("<rect width=\"1\" height=\"1\"/>");
        }
        paths.push_str("</g></svg>");
        assert!(matches!(
            import_svg_many(paths.as_bytes()),
            Err(crate::svg::SvgError::TooComplex {
                what: "vector paths",
                ..
            })
        ));

        let mut exact_anchors = String::from("<svg width=\"1\" height=\"1\">");
        for path in 0..218 {
            exact_anchors.push_str("<path d=\"M0 0");
            for point in 1..300 {
                exact_anchors.push_str(&format!(" L{} {}", point, path));
            }
            exact_anchors.push_str("\"/>");
        }
        exact_anchors.push_str("<path d=\"M0 0");
        for point in 1..136 {
            exact_anchors.push_str(&format!(" L{} 218", point));
        }
        exact_anchors.push_str("\"/></svg>");
        assert_eq!(
            import_svg_many(exact_anchors.as_bytes())
                .expect("exact total anchor cap is accepted")
                .len(),
            219
        );

        let mut anchors = String::from("<svg width=\"1\" height=\"1\">");
        for path in 0..220 {
            anchors.push_str("<path d=\"M0 0");
            for point in 1..300 {
                anchors.push_str(&format!(" L{} {}", point, path));
            }
            anchors.push_str("\"/>");
        }
        anchors.push_str("</svg>");
        assert!(matches!(
            import_svg_many(anchors.as_bytes()),
            Err(crate::svg::SvgError::TooComplex {
                what: "vector anchors across paths",
                ..
            })
        ));
    }

    #[test]
    fn root_and_nested_groups_inherit_override_and_isolate_paint() {
        let svg = br##"<svg width="1" height="1" fill="#112233" fill-opacity="0.5" fill-rule="evenodd" stroke="#445566" stroke-opacity="0.25" stroke-width="2" stroke-linecap="butt" stroke-linejoin="miter" stroke-dasharray="2 3" stroke-dashoffset="1"><g stroke="#AABBCC" stroke-linecap="square"><g fill="none" fill-rule="nonzero"><path d="M0 0 L1 1"/></g></g><g><path d="M0 1 L1 0"/></g></svg>"##;
        let shapes = import_svg_many(svg).expect("group paint inheritance");
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[0].shape.fill, [0, 0, 0, 0]);
        assert_eq!(shapes[0].shape.fill_rule, crate::svg::FillRule::NonZero);
        let first = shapes[0].shape.stroke.as_ref().expect("first stroke");
        assert_eq!(first.color, [170, 187, 204, 64]);
        assert_eq!(first.width, 2.0);
        assert_eq!(first.cap, crate::vector::StrokeCap::Square);
        assert_eq!(first.join, crate::vector::StrokeJoin::Miter);
        assert_eq!(
            first.dash,
            crate::vector::StrokeDash::from_lengths(&[2.0, 3.0], 1.0).expect("dash")
        );

        assert_eq!(shapes[1].shape.fill, [17, 34, 51, 128]);
        assert_eq!(shapes[1].shape.fill_rule, crate::svg::FillRule::EvenOdd);
        let second = shapes[1].shape.stroke.as_ref().expect("second stroke");
        assert_eq!(second.color, [68, 85, 102, 64]);
        assert_eq!(second.cap, crate::vector::StrokeCap::Butt);
        assert_eq!(second.dash, first.dash);
    }

    #[test]
    fn group_inherit_and_empty_values_preserve_parent_properties() {
        let svg = br##"<svg width="1" height="1" fill="#010203" fill-opacity="0.5"><g fill="inherit" fill-opacity="inherit"><path fill="" d="M0 0 L1 1 Z"/></g><g fill="#A0B0C0"><path fill="inherit" d="M0 1 L1 0 Z"/></g></svg>"##;
        let shapes = import_svg_many(svg).expect("explicit inheritance");
        assert_eq!(shapes[0].shape.fill, [1, 2, 3, 128]);
        assert_eq!(shapes[1].shape.fill, [160, 176, 192, 128]);

        let skipped = br##"<svg width="1" height="1"><defs><g fill="#FFFFFF"><path d="M0 0 L1 1"/></g></defs><g><path d="M0 0 L1 1"/></g></svg>"##;
        assert_eq!(
            import_svg_many(skipped)
                .expect("unknown subtree is skipped")
                .len(),
            1
        );
        let one =
            br##"<svg width="1" height="1"><g fill="#010203"><path d="M0 0 L1 1 Z"/></g></svg>"##;
        assert_eq!(
            import_svg(one)
                .expect("single grouped path compatibility")
                .shape
                .fill,
            [1, 2, 3, 255]
        );
        let default_miter = br##"<svg width="1" height="1" stroke-miterlimit="4"><g stroke-miterlimit="inherit"><path stroke-miterlimit="4.0" d="M0 0 L1 1"/></g></svg>"##;
        assert!(import_svg(default_miter).is_ok());
    }

    #[test]
    fn root_group_and_path_presentation_effects_are_typed_refusals() {
        let cases: &[(&[u8], &str)] = &[
            (
                br#"<svg width="1" height="1" style="fill:none"><path d="M0 0"/></svg>"#,
                "root style",
            ),
            (
                br#"<svg width="1" height="1" opacity="0.5"><path d="M0 0"/></svg>"#,
                "root opacity",
            ),
            (
                br#"<svg width="1" height="1" display="none"><path d="M0 0"/></svg>"#,
                "root display",
            ),
            (
                br#"<svg width="1" height="1" visibility="hidden"><path d="M0 0"/></svg>"#,
                "root visibility",
            ),
            (
                br#"<svg width="1" height="1"><g style="fill:none"><path d="M0 0"/></g></svg>"#,
                "group style",
            ),
            (
                br#"<svg width="1" height="1"><g opacity="0.5"><path d="M0 0"/></g></svg>"#,
                "group opacity",
            ),
            (
                br#"<svg width="1" height="1"><g display="none"><path d="M0 0"/></g></svg>"#,
                "group display",
            ),
            (
                br#"<svg width="1" height="1"><g visibility="hidden"><path d="M0 0"/></g></svg>"#,
                "group visibility",
            ),
            (
                br#"<svg width="1" height="1"><g clip-path="url(#c)"><path d="M0 0"/></g></svg>"#,
                "group clip-path",
            ),
            (
                br#"<svg width="1" height="1"><g stroke-miterlimit="2"><path d="M0 0"/></g></svg>"#,
                "group stroke-miterlimit",
            ),
            (
                br#"<svg width="1" height="1" stroke-miterlimit="2"><path d="M0 0"/></svg>"#,
                "root stroke-miterlimit",
            ),
            (
                br#"<svg width="1" height="1"><path opacity="0.5" d="M0 0"/></svg>"#,
                "path opacity",
            ),
            (
                br#"<svg width="1" height="1"><path display="none" d="M0 0"/></svg>"#,
                "path display",
            ),
            (
                br#"<svg width="1" height="1"><path visibility="hidden" d="M0 0"/></svg>"#,
                "path visibility",
            ),
            (
                br#"<svg width="1" height="1"><path style="fill:none" d="M0 0"/></svg>"#,
                "path style",
            ),
            (
                br#"<svg width="1" height="1"><path clip-path="url(#c)" d="M0 0"/></svg>"#,
                "path clip-path",
            ),
            (
                br#"<svg width="1" height="1"><path mask="url(#m)" d="M0 0"/></svg>"#,
                "path mask",
            ),
            (
                br#"<svg width="1" height="1"><path filter="url(#f)" d="M0 0"/></svg>"#,
                "path filter",
            ),
            (
                br#"<svg width="1" height="1"><path marker-end="url(#a)" d="M0 0"/></svg>"#,
                "path marker-end",
            ),
            (
                br#"<svg width="1" height="1"><path vector-effect="non-scaling-stroke" d="M0 0"/></svg>"#,
                "path vector-effect",
            ),
            (
                br#"<svg width="1" height="1" clip-path="url(#c)"><path d="M0 0"/></svg>"#,
                "root clip-path",
            ),
            (
                br#"<svg width="1" height="1" mask="url(#m)"><path d="M0 0"/></svg>"#,
                "root mask",
            ),
            (
                br#"<svg width="1" height="1" filter="url(#f)"><path d="M0 0"/></svg>"#,
                "root filter",
            ),
            (
                br#"<svg width="1" height="1" marker-end="url(#a)"><path d="M0 0"/></svg>"#,
                "root marker-end",
            ),
            (
                br#"<svg width="1" height="1" vector-effect="non-scaling-stroke"><path d="M0 0"/></svg>"#,
                "root vector-effect",
            ),
            (
                br#"<svg width="1" height="1" paint-order="stroke"><path d="M0 0"/></svg>"#,
                "root paint-order",
            ),
            (
                br#"<svg width="1" height="1"><path paint-order="stroke" d="M0 0"/></svg>"#,
                "path paint-order",
            ),
            (
                br#"<svg width="1" height="1"><path stroke-miterlimit="2" d="M0 0"/></svg>"#,
                "path stroke-miterlimit",
            ),
        ];
        for (svg, needle) in cases {
            match import_svg(svg) {
                Err(crate::svg::SvgError::Unsupported { what }) => {
                    assert!(what.contains(needle), "{what} lacks {needle}")
                }
                _ => panic!("accepted or mistyped refusal for {needle}"),
            }
        }
        // An explicit nonzero root rule is the implemented default, not an
        // unsupported semantic change.
        let svg = br#"<svg width="1" height="1" fill-rule="nonzero"><path d="M0 0 L1 1 Z"/></svg>"#;
        assert!(import_svg(svg).is_ok());
    }

    #[test]
    fn path_effects_are_refused_before_path_data_is_parsed() {
        let svg = br#"<svg width="1" height="1"><path display="none" d="not a path"/></svg>"#;
        match import_svg(svg) {
            Err(crate::svg::SvgError::Unsupported { what }) => {
                assert!(what.contains("path display"), "{what}")
            }
            _ => panic!("geometry parse ran before the effect refusal"),
        }
    }

    #[test]
    fn self_closing_root_is_refused_without_path_content() {
        assert!(import_svg(br#"<svg width="1" height="1"/>"#).is_err());
        assert!(import_svg(br#"<svg width="1" height="1"/><path d="M0 0"/>"#).is_err());
    }

    #[test]
    fn unsupported_documents_and_anchor_floods_are_typed_refusals() {
        let cases: &[&[u8]] = &[
            br#"<svg width="1" height="1"><path d="M0 0"/><path d="M1 1"/></svg>"#,
            br#"<svg width="1" height="1"><path d="M0 0 M1 1"/></svg>"#,
            br#"<svg width="1" height="1"><path d="M0 0 A1 1 0 0 0 1 1"/></svg>"#,
            br#"<svg width="1" height="1"><path d="M0 0 L1 1 Z" fill-rule="winding"/></svg>"#,
            "<svg width=\"1\" height=\"1\"><path d=\"M0 0\" fill=\"#øøø\"/></svg>".as_bytes(),
        ];
        for svg in cases {
            assert!(
                matches!(
                    import_svg(svg),
                    Err(crate::svg::SvgError::Unsupported { .. })
                ),
                "accepted or mistyped unsupported {svg:?}"
            );
        }

        let mut d = String::from("M0 0");
        for index in 1..=crate::vector::MAX_VECTOR_ANCHORS {
            d.push_str(&format!(" L{index} 1"));
        }
        let svg = format!("<svg width=\"1\" height=\"1\"><path d=\"{d}\"/></svg>");
        assert!(matches!(
            import_svg(svg.as_bytes()),
            Err(crate::svg::SvgError::TooComplex {
                what: "anchors",
                limit: crate::vector::MAX_VECTOR_ANCHORS,
            })
        ));
    }
}
