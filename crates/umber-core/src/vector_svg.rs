//! Deterministic SVG export for one live vector layer (K5a).
//!
//! The document uses a 1×1 UV viewBox, the same direct x/y coordinate mapping
//! as K2's `device_point`. K5b import is deliberately separate; this module
//! only serializes the model and never reads or writes files.

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
/// paths never fill; closed paths use nonzero fill. Painted strokes always
/// carry K2's round cap and join.
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
    out.push_str("\" fill-rule=\"nonzero\"");

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
        out.push_str("\" stroke-linecap=\"round\" stroke-linejoin=\"round\"");
    } else {
        out.push_str(" stroke=\"none\"");
    }
    out.push_str("/>\n</svg>\n");
    Ok(out)
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

/// Nine fractional digits are enough to round-trip an f32 while keeping the
/// export stable. Trimming zeros and canonicalizing -0 keeps goldens readable.
fn number(value: f32) -> String {
    let mut text = format!("{value:.9}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
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
            fill: [255, 0, 128, 128],
            stroke: Some(Stroke {
                width: 0.01,
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
            "fill-rule=\"nonzero\" fill=\"#FF0080\" fill-opacity=\"0.501960814\" ",
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
            fill: [255, 255, 255, 255],
            stroke: Some(Stroke {
                width: 0.1,
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
                color: [255, 255, 255, 255],
            },
            Stroke {
                width: f32::NAN,
                color: [255, 255, 255, 255],
            },
            Stroke {
                width: 0.1,
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
}
