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
    } else {
        out.push_str(" stroke=\"none\"");
    }
    out.push_str("/>\n</svg>\n");
    Ok(out)
}

/// Imports the bounded K5b single-path SVG subset into one vector layer.
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
                color: [255, 255, 255, 255],
            },
            Stroke {
                width: f32::NAN,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                color: [255, 255, 255, 255],
            },
            Stroke {
                width: 0.1,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
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
    fn root_transform_is_applied_once_and_path_transform_is_refused() {
        let svg =
            br#"<svg width="2" height="2" transform="translate(1 0)"><path d="M0 0 L1 1"/></svg>"#;
        let params = import_svg(svg).expect("one root transform");
        assert_eq!(params.shape.anchors[0].pos, [0.5, 0.0]);
        assert_eq!(params.shape.anchors[1].pos, [1.0, 0.5]);

        let svg = br#"<svg width="2" height="2" transform="translate(1 0)"><path transform="translate(1 0)" d="M0 0 L1 1"/></svg>"#;
        assert!(matches!(
            import_svg(svg),
            Err(crate::svg::SvgError::Unsupported { .. })
        ));
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
    fn root_and_path_presentation_effects_are_typed_refusals() {
        let cases: &[(&[u8], &str)] = &[
            (
                br##"<svg width="1" height="1" fill="#fff"><path d="M0 0"/></svg>"##,
                "root fill",
            ),
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
                br##"<svg width="1" height="1" stroke="#fff"><path d="M0 0"/></svg>"##,
                "root stroke",
            ),
            (
                br#"<svg width="1" height="1" stroke-width="2"><path d="M0 0"/></svg>"#,
                "root stroke-width",
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
                "path transform/style",
            ),
            (
                br#"<svg width="1" height="1"><path stroke-dasharray="1 2" d="M0 0"/></svg>"#,
                "path stroke-dasharray",
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
                br#"<svg width="1" height="1" fill-opacity="0.5"><path d="M0 0"/></svg>"#,
                "root fill-opacity",
            ),
            (
                br#"<svg width="1" height="1" stroke-linecap="butt"><path d="M0 0"/></svg>"#,
                "root stroke-linecap",
            ),
            (
                br#"<svg width="1" height="1" stroke-linejoin="miter"><path d="M0 0"/></svg>"#,
                "root stroke-linejoin",
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
                br#"<svg width="1" height="1"><path stroke-dashoffset="1" d="M0 0"/></svg>"#,
                "path stroke-dashoffset",
            ),
            (
                br#"<svg width="1" height="1" paint-order="stroke"><path d="M0 0"/></svg>"#,
                "root paint-order",
            ),
            (
                br#"<svg width="1" height="1"><path paint-order="stroke" d="M0 0"/></svg>"#,
                "path paint-order",
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
            br#"<svg width="1" height="1"><g><path d="M0 0"/></g></svg>"#,
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
